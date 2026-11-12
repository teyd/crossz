// Release builds are tray apps; a console window would pop up next to the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    crossz::run();
}
