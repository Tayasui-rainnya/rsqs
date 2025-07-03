# rsqs

`rsqs` （Rust Screenshot & QRCode Scanner）是一个基于 Rust 的跨平台屏幕捕获与二维码识别工具，支持 Windows，Linux (X11/Wayland)（还没试，我等下就去试） 和 macOS（按理说支持，但我没 mac 我试不了）。rsqs 集成了屏幕截图、剪贴板操作、二维码扫描等功能，适合办公辅助。

## 功能特性

- 跨平台（按理说支持）
- 二维码识别
- 轻量化，无需安装
- 基本是个人都会用

## 安装与构建

1. **克隆仓库**
    ```sh
    git clone https://github.com/yourname/rsqs.git
    cd ./rsqs
    ```

2. **安装依赖**
    - Rust 工具链：[安装 Rust](https://www.rust-lang.org/tools/install)
    - Linux 依赖（没试过我不知道，xcap 需要这些依赖）：
        ```sh
        # Debian/Ubuntu
        sudo apt-get install pkg-config libclang-dev libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev
        # Alpine
        sudo apk add pkgconf llvm19-dev clang19-dev libxcb-dev libxrandr-dev dbus-dev pipewire-dev wayland-dev mesa-dev
        # ArchLinux
        sudo pacman -S base-devel clang libxcb libxrandr dbus libpipewire
        ```

3. **编译项目**
    ```sh
    cargo build --release
    ```

    然后快乐地打开自己编译的软件吧


## 许可证

本项目采用 [GNU GPL v3](LICENSE) 许可证。
