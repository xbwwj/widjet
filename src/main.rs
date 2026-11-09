use crate::app::App;

mod app;
mod js;
mod wayland;

fn main() {
    App::new().run();
}
