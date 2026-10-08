# VibeDiary

A simple personal diary / journal for Windows, written in Rust with [eframe/egui](https://github.com/emilk/egui).

Write down your thoughts each day, tag them with colour-coded categories, and see at a glance which days have entries on a month calendar.

## Features

- **Month calendar** with each day's entries shown as coloured pills (time + first line of the entry).
- **Multiple entries per day**, each recorded with the time it was added.
- **Categories** with their own colours. Add, rename, recolour and delete them from the Categories window. Renaming a category updates the entries that use it.
- **Popup windows** for creating and editing entries and for managing categories; both are resizable.
- **Light and dark mode** that follow your Windows system setting.
- **Password protection.** The diary is encrypted with a password you choose, and the app asks for it every time it starts. There is a **Lock** button to lock it again and a **Password** button to change it.
- **Day panel** on the right listing the selected day's entries, with Edit and Delete for each.

## Using the app

| Action | How |
| --- | --- |
| Select a day | Click it on the calendar |
| New entry | Double-click a day, or use **New entry** / **Add entry for this day** |
| Edit or delete an entry | Use the buttons on its card in the day panel |
| Change month | **◀** / **▶** in the header, or **Today** to jump back |
| Manage categories | **Categories** button in the header (press Enter to apply a rename) |

Deleting a category does not delete its entries; they are shown in grey.

## Running it

You need the [Rust toolchain](https://rustup.rs/). From the project folder:

```
cargo run
```

Use `cargo run --release` for an optimised build.

## Where your data is stored

Entries and categories are saved, encrypted, to `entries.json` in the project folder (the folder is fixed when the app is built, so it doesn't depend on where you launch it from). The file is listed in `.gitignore`, so your diary is not committed to git.

## Encryption

- Your password is stretched into a key with **Argon2id** (64 MiB, 3 passes), and the diary is sealed with **XChaCha20-Poly1305**, which also detects a wrong password or a damaged or tampered file.
- `entries.json` is a small JSON envelope holding the key-derivation settings, a random salt and nonce, and the ciphertext. Nothing in it reveals your entries.
- Every save uses a fresh nonce, is checked by decrypting it again, and replaces the old file atomically.
- **There is no password recovery.** If you forget the password, the diary can't be read.
- Passwords must be at least 8 characters.
- An unencrypted `entries.json` from an earlier version is offered for encryption the first time you run the app. It is replaced by the encrypted version and no plain-text backup is kept.
- Text held in memory while the app is unlocked is not protected. Use **Lock** if you step away.

Once decrypted, the data looks like this:

```json
{
  "categories": [
    { "name": "Personal", "color": [96, 165, 250] }
  ],
  "entries": [
    {
      "id": 1,
      "date": "2026-10-08",
      "added_at": "2026-10-08 16:11:27",
      "category": "Personal",
      "text": "Your thoughts for the day"
    }
  ]
}
```

## Project layout

- `src/main.rs` contains the application: the UI, lock screen and file handling.
- `src/vault.rs` contains the password-based encryption.
- `src/icon.rs` draws the app icon.
- `Cargo.toml` lists the dependencies: `eframe`, `chrono`, `serde`, `serde_json`, `argon2`, `chacha20poly1305`, `base64` and `getrandom`.
