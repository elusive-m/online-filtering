use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, column, row, text},
    Command, Element, Length, Subscription,
};
use pyo3::{types::IntoPyDict, PyResult, Python};
use serialport::SerialPort;
use std::{
    io::{self, Read, Write},
    mem,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

mod graph;
mod workers;
use graph::Graph;

#[cfg(windows)]
use serialport::COMPort as Serial;
#[cfg(not(windows))]
use serialport::TTYPort as Serial;

use super::{ports::Ports, Message::Filter as App};

#[derive(Debug)]
pub enum Message {
    ConnectionFailed,
    ConnectionEstablished {
        serial: Serial,
        sampling_interval: f32,
    },
    Graph(graph::Message),
    Refresh,
    Finish,
    Export,
}

enum State {
    Connecting {
        function: String,
        stop_time: f32,
    },

    Connected {
        /// Realtime graph
        graph: Graph,
        /// For signalling cancellation to reader and writer threads
        cancellation_token: Arc<AtomicBool>,
        /// Thread handles. [`Option`] used to side-step shared reference issues
        /// Reference: https://stackoverflow.com/questions/57670145/how-to-store-joinhandle-of-a-thread-to-close-it-later
        receiver: Option<JoinHandle<()>>,
        transmitter: Option<JoinHandle<()>>,
    },

    Errored,
}

pub struct Filter {
    state: State,
}

impl Filter {
    pub fn new(
        port_name: String,
        function: String,
        stop_time: f32,
    ) -> (Self, Command<super::Message>) {
        let future = async move {
            tokio::task::spawn_blocking(move || -> io::Result<_> {
                use std::time::Duration;
                let mut serial = serialport::new(port_name, crate::BAUD_RATE)
                    .timeout(Duration::from_secs(3))
                    .open_native()?;

                thread::sleep(Duration::from_millis(250));
                serial.write_all(crate::SYN)?;

                let mut buf = [0u8; mem::size_of::<u32>()];
                serial.read_exact(&mut buf)?;

                let sampling_frequency = u32::from_le_bytes(buf);
                tracing::info!("Sampling frequency: {sampling_frequency}");

                serial.set_timeout(Duration::from_millis(100))?;
                Ok((sampling_frequency, serial))
            })
            .await
            .expect("blocking task ran")
        };

        (
            Self {
                state: State::Connecting {
                    function,
                    stop_time,
                },
            },
            Command::perform(future, |result| match result {
                Ok((sampling_frequency, serial)) => Message::ConnectionEstablished {
                    serial,
                    sampling_interval: (sampling_frequency as f32).recip(),
                },

                Err(e) => {
                    tracing::error!("Unable to establish connection: {e}");
                    Message::ConnectionFailed
                }
            })
            .map(App),
        )
    }
}

impl Filter {
    pub fn update(&mut self, message: Message) -> Option<Ports> {
        match message {
            Message::ConnectionFailed => {
                self.state = State::Errored;
                None
            }

            Message::ConnectionEstablished {
                serial: rx,
                sampling_interval,
            } => {
                let tx = rx.try_clone_native().expect("successful split");
                let (time, unfiltered_data) = self.compute_tensors(sampling_interval);
                let unfiltered_data = Arc::new(unfiltered_data);

                let total_samples = unfiltered_data.len();
                let cancellation_token = Arc::new(AtomicBool::new(false));

                let (filtered_data, receiver) = workers::spawn_receiver(rx, total_samples);

                let transmitter = workers::spawn_transmitter(
                    tx,
                    Arc::clone(&unfiltered_data),
                    Arc::clone(&cancellation_token),
                );

                self.state = State::Connected {
                    graph: Graph::new(time, unfiltered_data, filtered_data),
                    cancellation_token,
                    receiver: Some(receiver),
                    transmitter: Some(transmitter),
                };

                None
            }

            Message::Finish => match &mut self.state {
                State::Connected {
                    cancellation_token,
                    receiver,
                    transmitter,
                    ..
                } => {
                    // Signal termination
                    cancellation_token.store(true, Ordering::Relaxed);

                    // Wait for threads to terminate
                    if let Some(transmitter) = transmitter.take() {
                        transmitter.join().expect("successful tx termination");
                    }

                    if let Some(receiver) = receiver.take() {
                        receiver.join().expect("successful rx termination");
                    }

                    Some(Ports::new())
                }

                State::Errored => Some(Ports::new()),

                State::Connecting { .. } => unreachable!(),
            },

            Message::Graph(message) => {
                let State::Connected { graph, .. } = &mut self.state else {
                    unreachable!();
                };

                graph.update(message);
                None
            }

            Message::Refresh => {
                let State::Connected { receiver, transmitter, .. } = &mut self.state else {
                    unreachable!()
                };

                if receiver.as_ref().map_or(false, JoinHandle::is_finished) {
                    let tx = transmitter.take().expect("tx thread");
                    let rx = receiver.take().expect("rx thread");

                    rx.join().expect("successful rx termination");
                    tx.join().expect("successful tx termination");
                }

                None
            }

            Message::Export => match &self.state {
                State::Connected {
                    graph,
                    receiver: None,
                    transmitter: None,
                    ..
                } => {
                    match graph.export() {
                        Ok(()) => tracing::info!("Exported outputs"),
                        Err(e) => tracing::error!("Unable to export: {e}"),
                    }

                    None
                }

                _ => unreachable!(),
            },
        }
    }

    pub fn view(&self) -> Element<'_, super::Message> {
        let title = text("Online filtering")
            .width(Length::Fill)
            .size(48)
            .horizontal_alignment(Horizontal::Center);

        let content: Element<'_, Message> = match &self.state {
            State::Connected {
                graph, receiver, ..
            } => {
                let finish = button(
                    text("Ok")
                        .width(Length::Fill)
                        .horizontal_alignment(Horizontal::Center),
                )
                .width(Length::Fill)
                .on_press(Message::Finish);

                let graph = graph.view();

                if receiver.is_none() {
                    let export = button(
                        text("Export")
                            .width(Length::Fill)
                            .horizontal_alignment(Horizontal::Center),
                    )
                    .width(Length::Fill)
                    .on_press(Message::Export);

                    column![
                        title,
                        graph,
                        row![finish, export].spacing(10).width(Length::Fill)
                    ]
                } else {
                    column![title, graph, finish]
                }
            }

            State::Errored => {
                let message = text("Unable to connect...")
                    .size(32)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .vertical_alignment(Vertical::Center)
                    .horizontal_alignment(Horizontal::Center);

                let button = button(
                    text("Ok")
                        .width(Length::Fill)
                        .horizontal_alignment(Horizontal::Center),
                )
                .width(Length::Fill)
                .on_press(Message::Finish);

                column![title, message, button]
            }

            State::Connecting { .. } => {
                let message = text("Establishing connection...")
                    .size(32)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .vertical_alignment(Vertical::Center)
                    .horizontal_alignment(Horizontal::Center);

                column![title, message]
            }
        }
        .height(Length::Fill)
        .padding(15)
        .spacing(20)
        .into();

        content.map(App)
    }

    pub fn subscription(&self) -> Subscription<super::Message> {
        use iced::time::{self, Duration};

        match &self.state {
            State::Connected {
                receiver: Some(_),
                transmitter: Some(_),
                ..
            } => time::every(Duration::from_micros(1_000_000 / crate::FPS))
                .map(|_| App(Message::Refresh)),

            _ => Subscription::none(),
        }
    }

    fn compute_tensors(&self, sampling_interval: f32) -> (Vec<f32>, Vec<f32>) {
        let State::Connecting { function, stop_time, .. } = &self.state else {
            panic!();
        };

        Python::with_gil(|py| -> PyResult<_> {
            let numpy = py.import("numpy")?;
            let locals = crate::NUMPY_IMPORTS
                .iter()
                .map(|&member| (member, numpy.getattr(member).expect("valid member")))
                .into_py_dict(py);

            locals.set_item("np", numpy)?;
            let t = {
                let code = format!("np.arange(0, {stop_time}, {sampling_interval})");
                py.eval(&code, None, Some(locals))?
            };

            locals.set_item("t", t)?;
            let f = py.eval(function, None, Some(locals))?;

            Ok((t.extract()?, f.extract()?))
        })
        .expect("vectors")
    }
}

impl Clone for Message {
    fn clone(&self) -> Self {
        match &self {
            Message::Finish => Message::Finish,
            Message::Export => Message::Export,
            Message::Graph(message) => Message::Graph(*message),
            _ => unreachable!(),
        }
    }
}
