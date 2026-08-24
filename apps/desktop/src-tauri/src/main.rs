// A console window flashing open at every login would be the most visible thing
// about a launcher whose whole claim is that it is invisible until called.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    takyon_lib::run()
}
