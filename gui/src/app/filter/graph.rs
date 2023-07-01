use iced::{
    alignment::Horizontal,
    widget::{button, column, row, slider, text},
    Element, Length,
};
use parking_lot::Mutex;
use plotters_iced::{Chart, ChartBuilder, ChartWidget};
use std::{fs::File, io, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    SwitchMode,
    SizeUpdated(f64),
    OffsetUpdated(f64),
}

/// Streaming or static modes for graph
enum Mode {
    /// Only the latest samples will be shown
    Streaming,
    /// Allows the user to view a portion of the graph
    Static {
        /// How many points to display
        size: usize,
        /// Window offset from the first sample
        offset: usize,
    },
}

pub struct Graph {
    /// Current graph mode
    mode: Mode,
    /// Time vector
    time: Vec<f32>,
    /// Received data
    filtered_data: Arc<Mutex<Vec<f32>>>,
    /// Unfiltered data
    unfiltered_data: Arc<Vec<f32>>,
}

#[derive(serde::Serialize)]
struct ExportedData<'a> {
    input: &'a Vec<f32>,
    output: &'a Vec<f32>,
}

impl Graph {
    pub fn new(
        time: Vec<f32>,
        unfiltered_data: Arc<Vec<f32>>,
        filtered_data: Arc<Mutex<Vec<f32>>>,
    ) -> Self {
        Self {
            time,
            filtered_data,
            unfiltered_data,
            mode: Mode::Streaming,
        }
    }
}

impl Graph {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::SwitchMode => {
                if matches!(self.mode, Mode::Streaming) {
                    self.mode = Mode::Static {
                        size: crate::MIN_WINDOW_SIZE,
                        offset: 0,
                    }
                } else {
                    self.mode = Mode::Streaming;
                }
            }

            Message::SizeUpdated(value) => {
                let Mode::Static { size, .. } = &mut self.mode else {
                    unreachable!();
                };

                assign(size, value);
            }

            Message::OffsetUpdated(value) => {
                let Mode::Static { offset, .. } = &mut self.mode else {
                    unreachable!();
                };

                assign(offset, value);
            }
        }
    }

    pub fn view(&self) -> Element<'_, super::Message> {
        let chart = ChartWidget::new(self)
            .height(Length::Fill)
            .width(Length::Fill);

        let mode = {
            let label = if matches!(self.mode, Mode::Streaming) {
                "Disable streaming"
            } else {
                "Enable streaming"
            };

            button(
                text(label)
                    .horizontal_alignment(Horizontal::Center)
                    .width(Length::Fill),
            )
            .on_press(Message::SwitchMode)
            .width(Length::Fill)
        };

        let content: Element<'_, Message> = match self.mode {
            Mode::Streaming => {
                column![chart, mode]
            }

            Mode::Static { size, offset } => {
                let total_samples = (self.filtered_data.lock().len() - 1) as f64;

                let offset = slider(0f64..=total_samples, offset as f64, Message::OffsetUpdated)
                    .width(Length::Fill);

                let window = slider(
                    crate::MIN_WINDOW_SIZE as f64..=total_samples,
                    size as f64,
                    Message::SizeUpdated,
                )
                .width(Length::Fill);

                let labels = column![text("Window size"), text("Window offset"),].spacing(10);

                let controls = column![window, offset,].spacing(10).width(Length::Fill);

                column![
                    chart,
                    column![mode, row![labels, controls].spacing(25)].spacing(10),
                ]
            }
        }
        .height(Length::Fill)
        .width(Length::Fill)
        .spacing(15)
        .into();

        content.map(super::Message::Graph)
    }

    pub fn export(&self) -> io::Result<()> {
        let file = File::create(crate::FILENAME)?;
        let contents = ExportedData {
            input: &self.unfiltered_data,
            output: &self.filtered_data.lock(),
        };

        serde_json::to_writer(file, &contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl Chart<Message> for Graph {
    type State = ();

    fn build_chart<DB: plotters_iced::DrawingBackend>(
        &self,
        _state: &Self::State,
        mut builder: ChartBuilder<'_, '_, DB>,
    ) {
        use plotters::prelude::*;

        let filtered = self.filtered_data.lock();
        let unfiltered = self.unfiltered_data.as_slice();
        let total_samples = filtered.len();

        if total_samples == 0 {
            return;
        }

        let start;
        let end;

        match self.mode {
            Mode::Streaming => {
                start = total_samples - total_samples.min(crate::STREAMING_WINDOW_SIZE);
                end = total_samples - 1;
            }

            Mode::Static { size, offset } => {
                start = total_samples.min(offset);
                end = (start + size).min(total_samples - 1);
            }
        }

        let mut chart = builder
            .x_label_area_size(24)
            .y_label_area_size(24)
            .margin(10)
            .build_cartesian_2d(self.time[start]..self.time[end], -5f32..5f32)
            .expect("built chart");

        chart
            .configure_mesh()
            .axis_style(WHITE)
            .label_style(("sans-serif", 18).into_font().color(&WHITE))
            .max_light_lines(0)
            .bold_line_style(WHITE.mix(0.30))
            .draw()
            .expect("drawn mesh");

        let time = &self.time[start..end];
        let output = time
            .iter()
            .zip(&filtered[start..end])
            .map(|(x, y)| (*x, *y));
        let input = time
            .iter()
            .zip(&unfiltered[start..end])
            .map(|(x, y)| (*x, *y));

        // Input
        {
            let color = CYAN;
            chart
                .draw_series(LineSeries::new(input, color.stroke_width(2)))
                .expect("drawn input")
                .label("Input")
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));
        }

        // Output
        {
            let color = YELLOW;
            chart
                .draw_series(LineSeries::new(output, color.stroke_width(2)))
                .expect("drawn output")
                .label("Output")
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));
        }

        // Legend
        {
            chart
                .configure_series_labels()
                .border_style(WHITE)
                .label_font(("sans-serif", 18).into_font().color(&WHITE))
                .background_style(BLACK)
                .position(SeriesLabelPosition::UpperRight)
                .draw()
                .expect("drawn legend");
        }
    }
}

fn assign(out: &mut usize, value: f64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let value = value as usize;

    *out = value;
}
