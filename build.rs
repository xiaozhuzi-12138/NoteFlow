fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    slint_build::compile("ui/app-window.slint").unwrap();

    #[cfg(target_os = "windows")]
    embed_windows_icon();
}

#[cfg(target_os = "windows")]
fn embed_windows_icon() {
    use std::path::Path;

    let icon_path = Path::new("assets/icon.ico");
    if !icon_path.exists() {
        println!("cargo:warning=未找到 assets/icon.ico，跳过 Windows 图标嵌入");
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon_path.to_string_lossy().as_ref());
    res.set("ProductName", "便签");
    res.compile().unwrap();
}
