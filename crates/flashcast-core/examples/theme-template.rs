//! 开发者用：输出完整的双模式主题模板，或校验主题包。
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("check") {
        let path = args
            .get(1)
            .expect("用法：theme-template check <主题文件或目录>");
        match flashcast_core::ThemeDocument::from_package_path(std::path::Path::new(path)) {
            Ok(document) if document.schema_version == 2 => {
                println!("主题「{}」通过 v2 校验", document.name)
            }
            Ok(_) => {
                eprintln!("新主题必须采用 v2 规范");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut document = flashcast_core::builtin_themes().remove(0);
    document.id = "example.paper".into();
    document.name = "纸面".into();
    document.version = "1.0.0".into();
    document.styles = vec![flashcast_core::SurfaceStyle {
        id: "paper".into(),
        name: "纸面".into(),
        ..flashcast_core::SurfaceStyle::solid()
    }];
    document.default_style = Some("paper".into());
    document.validate().expect("模板必须有效");
    println!("{}", document.to_json().unwrap());
}
