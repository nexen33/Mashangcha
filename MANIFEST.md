# “码上查” (Mashangcha) v1.0.0 开源最小上传文件清单

- **总文件数**: 38 个
- **代码与资源总大小**: 625.96 KB (640978 字节)
- **过滤规则**: 已剔除 `Archiev/` 设计草稿与过程脚本、`target/` 二进制构建产物、`.git_env/` 本地工具、`enterprise.dat` 本地数据与临时缓存。

## 详细文件清单与 SHA-256 校验

| 序号 | 相对路径 | 文件大小 (Bytes) | SHA-256 前8位 |
| :--- | :--- | :--- | :--- |
| 1 | `.cargo/config.toml` | 128 | `5b12fe8c` |
| 2 | `.gitignore` | 253 | `873fcce9` |
| 3 | `app/build.rs` | 8,799 | `1cb41ee4` |
| 4 | `app/Cargo.toml` | 430 | `51ab077c` |
| 5 | `app/src/main.rs` | 4,316 | `61aaf66a` |
| 6 | `app/src/platform.rs` | 25,249 | `d20e1459` |
| 7 | `app/src/state.rs` | 33,365 | `8de5aaba` |
| 8 | `app/ui/icons/alert-triangle.svg` | 362 | `16c25fb6` |
| 9 | `app/ui/icons/app-icon.png` | 62,973 | `2c9ed44b` |
| 10 | `app/ui/icons/app.ico` | 165,123 | `02fc337b` |
| 11 | `app/ui/icons/check.svg` | 224 | `ab1e39bb` |
| 12 | `app/ui/icons/copy.svg` | 316 | `e73b737c` |
| 13 | `app/ui/icons/download.svg` | 325 | `06fc4537` |
| 14 | `app/ui/icons/drop-border.svg` | 222 | `fd9e93b2` |
| 15 | `app/ui/icons/minus.svg` | 210 | `fb3c4355` |
| 16 | `app/ui/icons/pin-off.svg` | 393 | `7439a081` |
| 17 | `app/ui/icons/pin.svg` | 394 | `b3ca6b80` |
| 18 | `app/ui/icons/search.svg` | 269 | `a629d287` |
| 19 | `app/ui/icons/settings.svg` | 801 | `74186927` |
| 20 | `app/ui/icons/topbar-logo.png` | 1,300 | `587b6b11` |
| 21 | `app/ui/icons/upload.svg` | 322 | `97c65e4f` |
| 22 | `app/ui/icons/x.svg` | 267 | `1d9e446c` |
| 23 | `app/ui/main.slint` | 70,470 | `d55bb89f` |
| 24 | `Cargo.lock` | 103,137 | `b3a6a771` |
| 25 | `Cargo.toml` | 1,571 | `175db221` |
| 26 | `core/Cargo.toml` | 521 | `d8885a3a` |
| 27 | `core/src/clean.rs` | 8,450 | `42159ad4` |
| 28 | `core/src/crypto.rs` | 7,485 | `5ff9263b` |
| 29 | `core/src/export.rs` | 4,745 | `14cf0ba8` |
| 30 | `core/src/import.rs` | 21,215 | `13d9758b` |
| 31 | `core/src/lib.rs` | 95 | `756d9bf0` |
| 32 | `core/src/model.rs` | 5,797 | `991efbef` |
| 33 | `core/src/search.rs` | 6,653 | `4ba8b861` |
| 34 | `LICENSE` | 33,788 | `fb86de72` |
| 35 | `README.md` | 16,767 | `8f33ae78` |
| 36 | `README_en.md` | 17,902 | `1fbcf2d5` |
| 37 | `rust-toolchain.toml` | 87 | `e43c5527` |
| 38 | `user_manuell.html` | 36,254 | `e3564ce2` |

## 目录结构拓扑图

```text
Mashangcha/
├── .cargo/
│   └── config.toml
├── app/
│   ├── src/                 # 前台 UI 绑定、主窗口生命周期与快捷键接管
│   ├── ui/                  # Slint 矢量 UI 定义与高清图标资源
│   ├── build.rs             # Slint 界面编译脚本
│   └── Cargo.toml
├── core/
│   ├── src/                 # 高速检索内核、拼音索引、Excel 流式导入/导出引擎
│   └── Cargo.toml
├── Cargo.lock
├── Cargo.toml               # 工作区 Cargo 声明
├── rust-toolchain.toml      # 固定 Rust 2021 工具链
├── .gitignore
├── LICENSE                  # GNU General Public License v3.0
├── README.md                # 中文版项目说明与架构文档
├── README_en.md             # 英文版项目说明与架构文档
└── user_manuell.html        # GitHub Pages 官方在线使用手册
```
