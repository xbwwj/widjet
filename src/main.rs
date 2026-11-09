use crate::app::App;

mod app;
mod canvas;
mod js;
mod wayland;

fn main() {
    App::new().run();
}
