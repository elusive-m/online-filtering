use iced::{
    alignment::Horizontal,
    widget::{
        button, column, horizontal_space, radio, row, scrollable, slider, text, text_input,
        vertical_space,
    },
    Command, Element, Length, Subscription,
};
use pyo3::{types::IntoPyDict, PyResult, Python};
use serialport::SerialPortInfo;

use super::{filter::Filter, Message::Ports as App};

#[derive(Debug, Clone)]
pub enum Message {
    RefreshPorts,
    PortSelected(usize),
    StopTimeUpdated(f32),
    FunctionUpdated(String),
    EvaluateFunction,
    Filter,
}

pub struct Ports {
    /// Function to be evaluated
    ///
    /// Evaluated at uniform intervals between \[0, [`Self::stop_time`]\]
    function: String,
    /// Is [`Self::function`] syntactically correct?
    validated: bool,
    /// How long to simulate [`Self::function`] for
    stop_time: f32,
    /// Index of desired port in [`Self::available_ports`]
    selected_port: Option<usize>,
    /// Scanned ports
    available_ports: Vec<SerialPortInfo>,
}

impl Ports {
    pub const fn new() -> Self {
        Self {
            function: String::new(),
            validated: false,
            stop_time: 1.0f32,
            selected_port: None,
            available_ports: Vec::new(),
        }
    }
}

impl Ports {
    pub fn update(&mut self, message: Message) -> Option<(Filter, Command<super::Message>)> {
        match message {
            Message::RefreshPorts => {
                self.update_ports(serialport::available_ports().unwrap_or_default());
                None
            }

            Message::PortSelected(i) => {
                self.selected_port = Some(i);
                None
            }

            Message::StopTimeUpdated(t) => {
                self.stop_time = t;
                None
            }

            Message::FunctionUpdated(f) => {
                self.function = f;
                self.validated = false;
                None
            }

            Message::EvaluateFunction => {
                self.validate();
                None
            }

            Message::Filter => {
                use std::mem::take;
                let i = self.selected_port.expect("selected port");

                Some(Filter::new(
                    take(&mut self.available_ports[i].port_name),
                    take(&mut self.function),
                    self.stop_time,
                ))
            }
        }
    }

    pub fn view(&self) -> Element<'_, super::Message> {
        let Self {
            function,
            validated,
            stop_time,
            selected_port,
            available_ports,
        } = self;

        let title = text("Online filtering")
            .width(Length::Fill)
            .size(48)
            .horizontal_alignment(Horizontal::Center);

        let stop_time_slider =
            slider(1.0f32..=30.0f32, *stop_time, Message::StopTimeUpdated).step(0.5f32);

        let function_editor = row![
            text_input("...", function)
                .on_input(Message::FunctionUpdated)
                .on_submit(Message::EvaluateFunction),
            button("Accept").on_press(Message::EvaluateFunction),
        ]
        .width(Length::Fill)
        .spacing(10);

        let ports = {
            let header = row![
                text("Available ports"),
                horizontal_space(Length::Fill),
                button("Refresh").on_press(Message::RefreshPorts),
            ]
            .width(Length::Fill);

            let ports: Element<'_, _> = if available_ports.is_empty() {
                text("No ports found").into()
            } else {
                let radios = available_ports
                    .iter()
                    .enumerate()
                    .map(|(i, SerialPortInfo { port_name, .. })| {
                        radio(port_name, i, *selected_port, Message::PortSelected)
                            .width(Length::Fill)
                            .into()
                    })
                    .collect();

                column(radios).width(Length::Fill).spacing(10).into()
            };

            column![header, scrollable(ports)].spacing(5)
        };

        let mut filter = button(
            text("Start filtering")
                .width(Length::Fill)
                .horizontal_alignment(Horizontal::Center),
        )
        .width(Length::Fill);

        if selected_port.is_some() && *validated {
            filter = filter.on_press(Message::Filter);
        }

        let content: Element<'_, Message> = column![
            title,
            column![
                column![text("f(t)").size(24), function_editor].spacing(10),
                column![
                    text(format!("Stop time [{stop_time:.2}]")).size(24),
                    stop_time_slider,
                ]
                .spacing(10),
            ]
            .spacing(15),
            ports,
            vertical_space(Length::Fill),
            filter
        ]
        .padding(15)
        .spacing(60)
        .into();

        content.map(App)
    }

    #[allow(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<super::Message> {
        use iced::time::{self, Duration};

        time::every(Duration::from_secs(3)).map(|_| App(Message::RefreshPorts))
    }

    fn update_ports(&mut self, mut ports: Vec<SerialPortInfo>) {
        if ports.is_empty() {
            self.selected_port = None;
            self.available_ports.clear();
            return;
        }

        let port_disconnected = self
            .selected_port
            .and_then(|i| self.available_ports.get(i))
            .map_or(false, |port| !ports.contains(port));

        if port_disconnected {
            self.selected_port = None;
        }

        // Retain new ports only
        ports.retain(|port| !self.available_ports.contains(port));

        self.available_ports.extend(ports);
    }

    fn validate(&mut self) {
        let Self {
            function,
            validated,
            ..
        } = self;

        let result = Python::with_gil(|py| -> PyResult<_> {
            let numpy = py.import("numpy")?;
            let locals = crate::NUMPY_IMPORTS
                .iter()
                .map(|&member| (member, numpy.getattr(member).expect("valid member")))
                .into_py_dict(py);

            locals.set_item("np", numpy)?;
            locals.set_item("t", py.eval("np.array([0])", None, Some(locals))?)?;

            py.eval(function, None, Some(locals)).map(|_| ())
        });

        if let Err(e) = result {
            tracing::error!("Evaluation failed: {e}");
            *validated = false;
        } else {
            tracing::info!("Evaluation successful");
            *validated = true;
        }
    }
}
