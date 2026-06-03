# MicKey — Microphone Mute Manager

**Version:** 2026.6.1

---

## What is MicKey?

MicKey is a small Windows program written in Rust that sits in your system tray and lets you mute and unmute your microphone quickly using a keyboard shortcut. A small colored icon stays on your screen at all times so you always know whether your mic is live or muted, without having to check any other app.

There is no installer. Just run `MicKey.exe` and it starts working immediately. The first time it runs, it creates a settings file (`mickey.ini`) in the same folder as the `.exe`. All your preferences are saved there automatically. MicKey communicates only with Windows itself (the audio system). It has no internet connection and stores nothing outside of `mickey.ini`.

---

## Getting Started

1. Place `MicKey.exe` in any folder you like.
2. Double-click `MicKey.exe` to launch it. A small colored dot or icon will appear in one corner of your screen. MicKey will also appear in the system tray (the icons near the clock).
3. Right-click the tray icon and choose **Settings** to configure your hotkey and adjust how the indicator looks.
4. To have MicKey start automatically with Windows, place a shortcut to `MicKey.exe` in your Windows Startup folder. You can open that folder by pressing `Win+R`, typing `shell:startup`, and pressing Enter.
5. MicKey works on Windows 7 and later, including all versions of Windows 10 and 11. Windows 7 and 8.1 will work but multi-monitor DPI scaling may not be pixel-perfect on displays with mismatched resolutions.

---

## How to Use MicKey

| Action | Result |
|---|---|
| **Left-click** the tray icon | Toggle your mic muted / unmuted instantly |
| **Right-click** the tray icon | Opens a menu with Settings, Toggle options, and Exit |
| **Hotkey** | Press your configured keyboard shortcut from anywhere |

**Right-click menu options:**
- **Settings** — Opens the full settings window.
- **Toggle Tray Icon** — Show or hide the tray icon.
- **Toggle Overlay** — Show or hide the on-screen indicator.
- **Exit** — Closes MicKey completely.

---

## On-Screen Indicator (Overlay)

MicKey displays a small colored icon pinned to a corner of your screen. The color tells you your mic state at a glance.

| Color | Meaning |
|---|---|
| 🟢 **Green** | Mic is **active** — people can hear you |
| 🔴 **Red** | Mic is **muted** — people cannot hear you |
| ⚫ **Gray** | **Error** — MicKey cannot reach your microphone |

The indicator stays on top of all other windows and does not interfere with clicks or typing. You can resize it, reposition it, change its color, or hide it entirely from Settings.

---

## Settings

Open Settings by right-clicking the tray icon and choosing **Settings**. Changes preview live on screen. Nothing is permanent until you click **Save Changes**.

<img width="506" height="485" alt="image" src="https://github.com/user-attachments/assets/1e12b8e1-1dda-4881-a9d8-7b4a1c434abc" />

### Input Device
Selects which microphone MicKey controls. **Default** uses whatever Windows has set as your default communication device (recommended for most people). If you have multiple microphones, pick the specific one you want from the list.

### Show Overlay
Checkbox. Tick to show the on-screen colored indicator; untick to hide it (the tray icon will still show your mic state).

### Show Tray Icon
Checkbox. Tick to show the MicKey icon near the system clock. **Important:** if you hide *both* the overlay and the tray icon, you will have no visual indicator at all. MicKey will still work, but you won't be able to see its menu unless you re-enable one of them by editing `mickey.ini` directly.

### Global Hotkey
The keyboard shortcut that controls your mic from anywhere.

1. Click the **Record** button.
2. Press the key combination you want (e.g., `Ctrl+Shift+M`).
3. The box will fill in automatically. Recording stops on its own.

Click **Stop** to cancel recording without changing the hotkey. The hotkey is only saved when you click **Save Changes**.

### Hotkey Mode

| Mode | Behavior |
|---|---|
| **Toggle** | Each press flips your mic on or off. Good for calls where you mostly stay muted and unmute to speak. |
| **Push-to-Talk** | Hold the key to unmute; release to mute again. Good if you want to stay muted except when speaking. |
| **Push-to-Mute** | Hold the key to mute; release to unmute. Good if you stay unmuted but want a quick-silence key. |

### Size *(px, max 64)*
The width and height of the on-screen indicator in pixels. Default is `24`. Use the arrows or type a number between `1` and `64`.

### Corner
Which corner of your screen the indicator appears in: `Top-Left` / `Top-Right` / `Bottom-Left` / `Bottom-Right`

### Offset X / Offset Y
How many pixels from the chosen corner the indicator is placed. `0` means flush with the edge; increase these values to move it inward. For multi-monitor setups, both positive and negative offset values are supported, from `-9999` to `9999`.

### Shape
| Value | Description |
|---|---|
| **MicKey Icon** | A microphone icon with active/muted variants |
| **Circle** | A plain filled circle |
| **Square** | A plain filled square |

### Colors
Click the colored swatch next to **Active**, **Muted**, or **Error** to open a color picker. The slider and the `%` box control opacity: `0%` = invisible, `100%` = fully solid.

### Save Changes / Discard Changes
- **Save Changes** — Applies all settings permanently and writes them to `mickey.ini`.
- **Discard Changes** — Cancels everything you changed since opening Settings and restores the previous values.

---

## The Settings File (`mickey.ini`)

MicKey saves all its settings to a plain text file called `mickey.ini`, located in the same folder as `MicKey.exe`. You can open it in Notepad at any time.

Most people will never need to touch this file directly — the Settings window handles everything. However, if you accidentally hide both the overlay and the tray icon, you can edit `mickey.ini` to fix it.

### Example `mickey.ini`

```ini
[Settings]
show_tray_icon=true
show_overlay=true
glyph_size=24
glyph_corner=top-left
offset_x=8
offset_y=8
glyph_shape=icon
device_override=
hotkey_str=Ctrl+Shift+M
hotkey_mode=toggle
color_muted=FF0000
opacity_muted=100
color_unmuted=00FF00
opacity_unmuted=100
color_error=808080
opacity_error=100
```

### Setting Reference

| Key | Description |
|---|---|
| `show_tray_icon` | `true` / `false` — show or hide the tray icon |
| `show_overlay` | `true` / `false` — show or hide the on-screen indicator |
| `glyph_size` | Size of the overlay in pixels (`1`–`64`) |
| `glyph_corner` | `top-left`, `top-right`, `bottom-left`, or `bottom-right` |
| `offset_x` | Pixels from the left or right edge. Supports values from `-9999` to `9999` for multi-monitor setups. |
| `offset_y` | Pixels from the top or bottom edge. Supports values from `-9999` to `9999` for multi-monitor setups. |
| `glyph_shape` | `icon`, `circle`, or `square` |
| `device_override` | Leave empty to use Windows' default communication mic. Set automatically by the Settings window; not recommended to type by hand. |
| `hotkey_str` | Your shortcut, e.g. `Ctrl+Alt+M`. Modifiers: `Ctrl`, `Shift`, `Alt`, `Win`. Leave empty for no hotkey. |
| `hotkey_mode` | `toggle`, `ptt` (push-to-talk), or `ptm` (push-to-mute) |
| `color_muted` / `color_unmuted` / `color_error` | 6-character hex color code (e.g. `FF0000` = red, `00FF00` = green) |
| `opacity_muted` / `opacity_unmuted` / `opacity_error` | `0`–`100`. `100` = fully visible, `0` = invisible. |

---

## Building from Source

Requires [Rust](https://rustup.rs/) (stable toolchain) and a Windows build environment.

```bash
git clone https://github.com/ForFunGplDev/MicKey.git
cd MicKey
cargo build --release
```

The compiled binary will be at `target\release\MicKey.exe`.

---

## FAQ

**Q: MicKey shows a gray indicator and won't mute my mic.**
MicKey can't find your microphone. Make sure it's plugged in and that Windows recognizes it. Open Settings and check that the correct device is selected under **Input Device**.

**Q: My hotkey doesn't seem to work.**
Make sure you clicked **Save Changes** after recording the hotkey — it is not saved automatically. Also check that no other application is already using the same key combination.

**Q: The indicator overlaps something important on my screen.**
Open Settings and adjust **Corner**, **Offset X**, and **Offset Y** to move it somewhere out of the way. You can also reduce **Size** to make it smaller.

**Q: I want the indicator fully invisible but still want the hotkey to work.**
Set opacity to `0` for the states you want invisible, or uncheck **Show Overlay** in Settings. The hotkey will continue to work either way.

**Q: I accidentally hid both the overlay and the tray icon. How do I get back?**
Open `mickey.ini` in Notepad and set both of these lines to `true`:
```ini
show_tray_icon=true
show_overlay=true
```
Save the file, then close (via Task Manager or restart) and reopen `MicKey.exe`.

**Q: Does MicKey need to run as Administrator?**
No. It runs with normal user permissions.

---

## Closing MicKey

Right-click the tray icon and choose **Exit**.

If the tray icon is hidden, open Task Manager (`Ctrl+Shift+Esc`), find **MicKey** in the process list, and click **End Task**.

---

## License

MicKey is free and open-source software, released under the [GNU General Public License v3.0](LICENSE).
