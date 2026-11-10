<div align="right">
  <a href="./README.md">简体中文</a> | <strong>English</strong>
</div>

<div align="center">
  <img src="https://github.com/user-attachments/assets/c7ccf9c7-7dea-46b4-b157-803f9f22c7e8" width="76" height="76" alt="Mashangcha Logo" style="border-radius: 16px;" />
  <h1>Mashangcha (码上查)</h1>

  <p>
    <img src="https://img.shields.io/badge/Platform-Windows%207%20%7C%2010%20%7C%2011%20(x86%20%2F%20x64)-blue" alt="Platform" />
    <img src="https://img.shields.io/badge/Rust-1.77.2%20(Win7%20Compatible)-orange" alt="Rust" />
    <img src="https://img.shields.io/badge/UI-Slint%20Software%20Render-green" alt="Slint" />
    <img src="https://img.shields.io/badge/Build-Single%20Binary%20(Static%20CRT)-purple" alt="Build" />
    <img src="https://img.shields.io/badge/License-GPL--3.0-blue" alt="License" />
    <img src="https://img.shields.io/badge/Version-v1.0.0-emerald?color=059669" alt="Version" />
  </p>

  <p>
    <strong>A lightweight floating desktop lookup tool for corporate billing & tax codes, tailored for accountants and administrative personnel</strong><br />
    <em>Non-intrusive floating overlay · Zero-focus-loss copy · Portable & offline · Compatible with Windows 7 (32-bit) and above</em>
  </p>

  <p align="center">
    <a href="https://nexen33.github.io/Mashangcha/user_manuell.html"><img src="https://img.shields.io/badge/Read_Online-User_Manual-white?style=for-the-badge&logo=readme&logoColor=047857" height="26" alt="User Manual" /></a>&nbsp;&nbsp;<a href="https://github.com/nexen33/Mashangcha/releases/download/v1.0.0/Mashangcha_v1.0.0.exe"><img src="https://img.shields.io/badge/Download-Mashangcha_v1.0.0.exe-047857?style=for-the-badge&logo=windows&logoColor=white" height="26" alt="Download" /></a>
  </p>
</div>

---

<p align="center">
  <img src="https://github.com/user-attachments/assets/f1a269ef-a926-40bb-b277-5ca2fbbedd4c" alt="Mashangcha Interface Preview" width="100%" />
</p>

---

## Background & Pain Points: What is Mashangcha?

If you have ever worked in accounting, corporate invoicing, or administrative billing, you are likely all too familiar with this frustrating daily routine:

When issuing VAT invoices or recording bookkeeping entries in legacy financial or tax invoicing software (such as Aisino/Hangxin, Baiwang, Yonyou, Kingdee, etc.), the built-in dropdown selection boxes are notoriously rigid. They rarely support pinyin acronyms (initials), phonetic abbreviations, or space-separated multi-keyword queries. With hundreds or thousands of clients and vendors to manage, accountants often keep a massive Excel spreadsheet running in the background, repeatedly pressing `Ctrl + F` to search, carefully double-clicking to copy client codes and tax IDs, and then switching back to paste.

Constantly toggling between windows is not only tedious and error-prone, but Excel cell formatting often corrupts long numeric codes into floating-point scientific notation (`1.2345E+17`) or copies invisible trailing whitespace, leading to voided tax invoices. To make matters worse, many corporate workstations are decade-old Windows 7 machines lacking dedicated GPU drivers, where modern applications frequently crash with missing runtime errors or unlocated entry points.

**Mashangcha (码上查) was created to eliminate this mechanical friction once and for all.**

It acts like an ultra-lightweight "sticky note" that quietly docks beside your invoicing software. Without ever leaving your current invoicing screen, typing a few pinyin letters instantly reveals the company. A single mouse click or hitting <kbd>Enter</kbd> sends the corporate code straight to your clipboard, and you can immediately press `Ctrl + V` to paste—eliminating even the extra click needed to refocus the input field!

### Clarifications: What Mashangcha is NOT

To avoid any misunderstanding before use:
- **It is NOT a cloud financial service and never connects to the internet**: The software runs 100% locally and offline, with no remote servers, no telemetry, and no data collection.
- **It does NOT inject or tamper with invoicing software**: No DLL injection, no memory hooking, and zero interference with tax control system integrity.
- **It is NOT an Electron/WebView bloatware**: Built purely on native system primitives in Rust, packing all logic and UI into a single standalone `.exe` binary of only ~4.4 MB.

---

## Key Features

### 1. Docked Floating Overlay with "Zero Focus Loss" (Signature Feature)
- **Always-on-Top Floating**: Click the pin icon in the upper-right corner to keep the window floating above all full-screen accounting applications.
- **Non-Stealing Input Cursor Focus**: While your cursor is actively blinking inside the invoicing software's code input field, directly clicking a card in Mashangcha **will NOT steal the keyboard input focus from the invoicing application!** You can immediately press `Ctrl + V` on your physical keyboard to paste, doubling your input efficiency.
- **Blind-Typing with Enter**: After typing your query, you don't even need to touch the mouse—simply press <kbd>Enter</kbd> to copy the top-ranked match to your clipboard.

### 2. Pinyin Initials & Multi-Keyword Flexible Search
- **Pinyin Initial Acronyms**: Searching for a company like "北京京东世纪贸易有限公司" requires no Chinese input method switching; simply type `jd` or `bjjdsj` to immediately surface it as the top result.
- **Space-Separated Multi-Keywords**: When dealing with numerous subsidiaries or regional branches, type `华为 销售` (with a space) to dynamically filter entries containing both keywords.
- **Fuzzy Code and Tax ID Lookup**: If you only remember the last few digits of a client code (e.g., `688`) or a tax ID segment, typing the numbers directly brings up the match.

### 3. Drag-and-Drop Excel / CSV Import with Smart Column Alignment
- **Input / Output Tax Separation**: The application maintains strict physical data isolation between **[Input Tax]** (Vendors) and **[Output Tax]** (Clients). Import operations strictly apply to the currently active tab! Before dragging or selecting files, **always verify the sliding toggle at the top center to confirm whether you are on [Input] or [Output]**. Importing input entries will never contaminate or overwrite output entries, and vice versa.
- **Drag-and-Drop Simplicity**: Once the category is verified, drag any `.xlsx`, `.xls`, or `.csv` file directly into the application window to complete ingestion in one second.
- **Smart Header Detection**: Regardless of whether your headers are named "Client Name", "Vendor Title", "Company Code", "Customer ID", or "Unified Social Credit Code", the engine automatically detects and aligns columns without requiring prior file modifications.
- **Headerless Raw Data Fallback**: If an exported sheet lacks a header row entirely, the engine seamlessly falls back to the standard convention: `Column 1: Company Name`, `Column 2: Corporate Code`, `Column 3: Tax ID (Optional)`.

### 4. Strict Error Prevention & One-Click Conflict Copying
- **Scientific Notation Interception**: If Excel cell formatting has corrupted a numeric code into a floating-point number like `1.23E+17`, the engine immediately halts and rejects the corrupted entry to keep your database pristine.
- **Zero-Tolerance Duplicate Interception**: Invoicing systems strictly prohibit duplicate company names or conflicting codes. If duplicates are found, Mashangcha highlights the exact conflicting rows. Clicking **[Copy All Conflicts]** lets you paste the detailed report back into Excel for instant verification.

### 5. Arbitrary Window Stretching & Adaptive Memory
- Height expands from the default **3 rows** up to **10 rows**; width expands up to **150%** to accommodate exceptionally long state-owned enterprise names without awkward line wrapping.
- Window geometry is automatically encrypted and saved upon mouse release, perfectly restoring your preferred size on subsequent launches.

### 6. Portable USB Drive Mode & Hardware-Bound Encryption
- **Default Host Mode**: Imported data is authenticated and encrypted on disk using `ChaCha20-Poly1305` bound to your local PC's hardware fingerprint. Copying the data file to another machine renders it completely unreadable, safeguarding commercial privacy.
- **Portable USB Mode**: Creating a `portable.flag` file in the same directory as the executable directs all encrypted data to be stored locally alongside the program, making it easy to carry on a flash drive between workstations.

---

## System Requirements & Extreme Legacy Compatibility

Many accounting offices still rely on legacy Windows 7 computers lacking dedicated GPU drivers and VC++ runtime redistributables. Mashangcha was engineered from line one for extreme backwards compatibility:

| Dimension | Specification & Technical Implementation |
| :--- | :--- |
| **Operating System** | **Windows 7** (SP1 Barebone Ultimate/Pro), **Windows 8 / 10 / 11** (Full 32-bit and 64-bit support) |
| **Target Architecture** | Native **32-bit (x86 / i686)** PE binary, runs natively on 32-bit OS and seamlessly on 64-bit OS via WOW64 |
| **Runtime Dependencies** | **Zero**. Statically linked with C-Runtime (`+crt-static`), requiring no VC++ Redistributables or .NET Framework |
| **Graphics & GPU** | **Pure CPU Software Rasterization**. Completely independent of Direct3D, OpenGL, or Vulkan hardware acceleration; launches cleanly without crashes or visual glitches even inside basic virtual machines |
| **Distribution Format** | **Single standalone portable binary (`Mashangcha_v1.0.0.exe`)**, ~4.4 MB, no installer required, zero registry pollution |

---

## Installation & Getting Started

### 1. Recommended Deployment (D: Drive)
To prevent accidental data loss during operating system reinstallations, saving the application on the D: drive is recommended:
1. Download the latest `Mashangcha_v1.0.0.exe` binary and move it to your **D: drive** (e.g., inside `D:\Mashangcha\`).
2. Rename `Mashangcha_v1.0.0.exe` to a cleaner name: **`码上查.exe`** (or `Mashangcha.exe`).
3. Right-click on `码上查.exe` → select **[Send to]** → **[Desktop (create shortcut)]**.
4. Double-click the desktop shortcut anytime for instant launch!

### 2. First-Time Ingestion (3 Simple Steps)
1. Launch "码上查" and **check the sliding toggle at the top center to ensure you are on the intended category ([Input] or [Output])**.
2. Click the settings gear icon `[⚙️]` in the upper right, choose **[Import Data (Excel/CSV)]**, or **drag your spreadsheet directly into the window**.
3. Type a few pinyin letters in the search bar, and your matching corporate entries will appear instantly!

---

## Technical Architecture

The project adopts a decoupled architecture separating pure business logic from Win32 GUI presentation:

```text
┌─────────────────────────────────────────────────────────────┐
│                 Mashangcha (码上查) Desktop                 │
└──────────────────────────────┬──────────────────────────────┘
                               │
       ┌───────────────────────┴───────────────────────┐
       ▼                                               ▼
┌───────────────────────────────┐     ┌───────────────────────────────┐
│        mashangcha-core        │     │        mashangcha-app         │
│  (Pure Logic · Zero GUI Dep)  │     │   (GUI & Native Interactions) │
├───────────────────────────────┤     ├───────────────────────────────┤
│ • Smart Header & Multi-Enc CSV│     │ • Slint CPU Software Render   │
│ • Deep Text Clean & Space Fold│     │ • Win32 Non-Focus Subclassing │
│ • Fast Pinyin Initials Index  │     │ • 300ms Debounce & CAS Locks  │
│ • Sub-ms Multi-Keyword Ranker │     │ • WM_ERASEBKGND Anti-Flicker  │
│ • Float Scientific Notation   │     │ • Window Memory & 4-Layer PSS │
│ • ChaCha20-Poly1305 HW Crypto │     │ • PE Version & HiDPI Assets   │
└───────────────────────────────┘     └───────────────────────────────┘
```

---

## Privacy & Security Commitment

1. **Strictly Offline**: The codebase **contains zero network communication routines**. No background pings, no telemetry, no tracking, and no hidden analytics.
2. **Hardware-Fingerprint Encryption**: Data saved to disk is encrypted with keys derived from hardware-level machine characteristics, ensuring files copied off the host machine remain cryptographically locked.
3. **Green & Residue-Free**: Portable mode confines all operations within the application's local directory; deleting the folder completely removes the application without leaving registry residue.

---

## Frequently Asked Questions (FAQ)

#### Q1: Why does clicking a card not defocus my invoicing software?
**A: This is the intentional signature feature of Mashangcha!** In pinned overlay mode, clicking only transfers the code into the clipboard without stealing the operating system's keyboard focus. You can immediately press <kbd>Ctrl + V</kbd> inside your invoicing software without having to click back to refocus.

#### Q2: What should I do if an import is rejected due to "Duplicate company name or code"?
**A**: Bookkeeping requires strict account isolation. When the conflict warning appears, click **[Copy All Conflicts]** on the alert modal, paste it into Excel with <kbd>Ctrl + V</kbd> to inspect the conflicting row numbers, correct the entries, and re-import.

#### Q3: Will exported Excel files convert tax IDs into scientific notation?
**A**: Absolutely not. During export, Mashangcha forcibly injects `@` text formatting into all code and tax ID cells, ensuring that numbers never drop leading zeros or turn into scientific notation regardless of the Excel version used.

#### Q4: What does "Show all enterprises" mean in Settings? Why does "When unsearched" only display the top 20 entries?
**A: This represents a deliberate engineering balance between usability and extreme legacy hardware efficiency.**
- **Functional Behavior**:
  - **Default [Disabled]**: When the search query is empty, the viewport remains uncluttered, displaying only a centered shortcut prompt without rendering cards. The upper right corner displays the total count ("Total: N"). Cards appear instantly as soon as you type.
  - **Enabled [When Unsearched]**: When the search field is empty, the system lists the top 20 enterprise entries of the current category, allowing quick one-click copying for frequently accessed companies.
- **Why strictly limit to 20 items (Top-20 Truncation)?**
  - **CPU Software Rasterization Protection**: To ensure smooth 60fps operation on legacy Windows 7 machines lacking GPU drivers, Mashangcha utilizes a pure CPU software rasterization pipeline. Instantiating thousands of UI card widgets into the layout tree simultaneously would incur excessive memory overhead and cause CPU stuttering or freezes during scrolling.
  - **Instant Responsiveness**: Capping the unsearched view at 20 entries (enforced by `MAX_SEARCH_RESULTS = 20` in the engine) keeps memory allocation minimal and scrolling buttery-smooth while matching real-world accounting workflows—typing two pinyin letters is exponentially faster than manually scrolling through hundreds of rows. Once any keyword is entered, sub-millisecond ranking algorithms instantly search across the entire dataset to surface the exact match.

---

## Open Source License & Third-Party Notices

### 1. Project License
This project is licensed under the **GNU General Public License v3.0 (GPL-3.0)**. Copyright © 2026 **Tun&PaMa AG**.
- For full license terms, please refer to [LICENSE](./LICENSE).

**Special Notice**: Free for personal, academic, and commercial evaluation. If you distribute modified versions of this software, your derived source code must also be made available under GPL-3.0 with original copyright attribution preserved.

### 2. Third-Party UI Engine (Slint) Attribution & Derivative Disclaimer
The graphical interface of this project is built upon the **Slint** (https://slint.dev/) open-source toolkit (GPLv3 / Royalty-Free Desktop License), Copyright © SixtyFPS GmbH and Slint community contributors.
- This project strictly adheres to the GPLv3 licensing requirements mandated by Slint for open-source distribution;
- This project is an independent third-party open-source work and is not affiliated with, endorsed by, or sponsored by SixtyFPS GmbH. Slint's authors and contributors bear no warranty or legal liability regarding the runtime stability or business suitability of this derivative software.

---

## Disclaimer

1. **Non-Commercial Productivity Tool**: This software is an independently developed utility designed to improve data entry efficiency. Compatibility with all third-party financial software versions is not guaranteed.
2. **Trademarks & Copyrights**: Mentions of Aisino/Hangxin, Baiwang, Yonyou, Kingdee, and related product names are strictly for objective compatibility and contextual descriptions. All trademarks belong to their respective copyright holders. Mashangcha has no affiliation, sponsorship, or commercial endorsement with any of these organizations.
3. **Data Responsibility**: Users are responsible for maintaining backups of their original spreadsheets. The authors and copyright holders assume no liability for data loss or operational errors resulting from hardware malfunction or improper usage.

---

<p align="center">
  <em>助你做账无烦恼，码上查，马上办！</em><br />
  <em>Entwickelt mit ❤️ von Tun&PaMa Familie</em><br />
  <em>Copyright © 2026 Tun&PaMa AG. All rights reserved.</em>
</p>

