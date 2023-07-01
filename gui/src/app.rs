use iced::{executor, Application, Command, Element, Subscription, Theme};

mod filter;
use filter::Filter;
mod ports;
use ports::Ports;

enum State {
    Ports(Ports),
    Filter(Filter),
}

pub struct OnlineFiltering {
    state: State,
}

#[derive(Debug, Clone)]
pub enum Message {
    Ports(ports::Message),
    Filter(filter::Message),
}

impl Application for OnlineFiltering {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self {
                state: State::Ports(Ports::new()),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Online filtering".to_owned()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match (message, &mut self.state) {
            (Message::Ports(message), State::Ports(ports)) => {
                if let Some((filter, command)) = ports.update(message) {
                    self.state = State::Filter(filter);
                    return command;
                }
            }

            (Message::Filter(message), State::Filter(filter)) => {
                if let Some(ports) = filter.update(message) {
                    self.state = State::Ports(ports);
                }
            }

            _ => unreachable!(),
        }

        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match &self.state {
            State::Ports(ports) => ports.view(),
            State::Filter(filter) => filter.view(),
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        match &self.state {
            State::Ports(ports) => ports.subscription(),
            State::Filter(filter) => filter.subscription(),
        }
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}
