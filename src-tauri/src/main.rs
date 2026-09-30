// Windows 发布构建下不弹出控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    flashcast_lib::run()
}
