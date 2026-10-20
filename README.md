# esp32-s3-luanti

A minimal [Luanti](https://www.luanti.org/) (formerly Minetest) server that runs on an ESP32-S3 microcontroller, written in Rust on ESP-IDF.

A standard, unmodified Luanti client connects to it over Wi-Fi as it would to any server. You can create an account, log in, explore a generated world with hills, forests, caves, biomes and ores, dig and build, place torches, drop and pick up items, choose a skin, and see the other players. The world, accounts, inventories and player positions are stored on a microSD card. The board's small LCD shows the server's status.

The server does not run Lua. A normal Luanti server runs a game (such as Minetest Game) written in Lua mods. This one implements the engine's network protocol directly and builds a small built-in game on top of it, in about 7,000 lines of Rust.

![The server running on a Waveshare ESP32-S3-LCD-1.47, with its status on the built-in screen](assets/server.jpeg)

## What it looks like

A Luanti client playing on the ESP32:

![In game, connected to the ESP32 server](assets/screen-001.png)

![In game, connected to the ESP32 server](assets/screen-002.png)

## Hardware

Developed on a [**Waveshare ESP32-S3-LCD-1.47**](https://www.waveshare.com/product/esp32-related/boards-kits/esp32-s3-lcd-1.47.htm):

| | |
|---|---|
| Chip | ESP32-S3, dual-core Xtensa LX7 at 240 MHz |
| RAM | 512 KB internal SRAM, plus 8 MB octal PSRAM |
| Flash | 16 MB |
| Display | 1.47" ST7789 LCD, 172 × 320 |
| Storage | microSD slot (4-bit SDMMC) |
| Network | 2.4 GHz Wi-Fi |

![The back of the board, with the microSD card that holds the world](assets/esp32-waveshare.jpeg)

Other ESP32-S3 boards with PSRAM and an SD card should work with pin changes. The SD card pins are set in `src/main.rs` (CMD 15, CLK 14, D0 16, D1 18, D2 17, D3 21) and the LCD pins in `src/lcd.rs`. PSRAM is required: the server keeps its own data there and leaves the internal RAM to Wi-Fi and the system.

## What works

- **Protocol:** the Luanti network protocol as spoken by Luanti 5.17 (protocol version 53), including the reliable-UDP layer with split packets, acknowledgements, resends and out-of-order delivery.
- **Accounts:** SRP-6a password login, the same scheme the real server uses. New players register on first connect, and passwords are never stored or sent.
- **World generation:**
  - rolling hills and mountains;
  - grassland, desert and snow biomes;
  - caves with entrances;
  - coal and iron ore;
  - birch woods in the lowlands and fir forests in the hills and snow, with clearings and the odd lone tree; leaves cast shade.
- **World edge:** the world ends about 31,000 nodes from the centre in every direction, at an invisible wall, as in Luanti.
- **World storage:** the world is generated on demand from a fixed seed. Only the changes players make are stored, as an append-only log on the SD card.
- **Digging and building:** changes appear for every player at once.
- **Survival inventory:** 32 slots, with stacks you can rearrange in the inventory window. New players start with 20 torches.
- **Torches:** they mount on floors, walls and ceilings using Minetest Game's 3D models. The server computes their light, so caves and nights are lit, and they fall off when the block they hang on is dug.
- **Day and night:** a normal day and night cycle. Map blocks carry separate day and night light.
- **Health:** fall damage, a health bar, death and respawn.
- **Dropped items:**
  - `Q` drops the whole stack in your hand; `Shift+Q` drops one;
  - punch an item to pick it up;
  - items fall, land, and merge with matching items nearby.
- **Up to six players at once:** a seventh is turned away with Luanti's "too many users" message. The limit is `MAX_PLAYERS` in `src/server.rs`; more players would overload the ESP32, mainly in generating map blocks when several explore at once.
- **Welcome message:** joining players are told what the server is and who else is on.
- **Chat:** with Luanti's usual limits (500 characters, 8 messages per 10 seconds), and the commands `/help`, `/me`, `/msg` (a private message), `/status` and `/time`.
- **Sounds:** Minetest Game's footstep, digging, breaking and placing sounds for each kind of block, and a sound when you are hurt. The client plays them; the server only says which.
- **Players:** other players are shown with the animated Minetest Game character and their name tags.
- **Skins:**
  - chosen with a live 3D preview in the inventory window;
  - extra skins can be added on the SD card;
  - 64×64 Minecraft skins are accepted.
- **Saving:** player positions, health, skins and inventories are saved and restored.
- **Status light:** while the screen is off, the board's RGB LED shows what is happening. It never goes above 60% of full power.
  - slow green breathing while the server runs, a little quicker with players on;
  - two blue flashes when a player joins, two purple when one leaves, and a white flash for chat;
  - three red flashes when something goes wrong, such as failing to save to the SD card;
  - amber blinking while looking for Wi-Fi.
- **Status screens:** the LCD has three pages, switched with the board's BOOT button. The screen switches off after a minute without a press, to keep the board cooler; the next press wakes it on the same page.
  - **Overview:** players online, packet and block counts, memory, and recent events.
  - **Device:** load on each CPU core, how busy the server is, time per map block, memory (free, lowest, largest piece), Wi-Fi signal and channel, failed sends, SD card space, and the chip's temperature.
  - **Chat:** the latest public chat, newest at the bottom. The screen font covers Latin-1 and Latin Extended-A, so Finnish, Swedish and most European letters show properly.

## What doesn't (yet)

- No crafting, no admin commands.
- No Lua mods: the game content is built in.
- Dropped items are not saved over a restart.
- One world per SD card.
- The time of day restarts at 10:00 when the server starts.
- Only tested with Luanti 5.17 clients. Older clients down to protocol 37 are accepted but untested.

## Building

You need the Rust toolchain for Xtensa, plus `git` and Python 3 with the `venv` module. The ESP-IDF build creates its own Python environment inside `.embuild/` on the first run; you don't need to set one up. On Ubuntu: `sudo apt install git python3 python3-venv`.

1. Install the Rust Xtensa toolchain with [espup](https://github.com/esp-rs/espup), then load its environment (in every new shell, or from your shell profile):
   ```
   cargo install espup
   espup install
   . ~/export-esp.sh
   ```
   `rust-toolchain.toml` selects that toolchain (`esp`) for this project.
2. Install the linker wrapper and the flashing tool:
   ```
   cargo install ldproxy espflash
   ```
3. Connect the board by USB, then build and flash. Give your Wi-Fi details as environment variables the first time. The firmware writes them to `wifi.txt` on the SD card when it starts (see [Wi-Fi networks](#wi-fi-networks)), and they are not stored in the repository.
   ```
   WIFI_SSID='your network' WIFI_PASS='your password' cargo run --release
   ```
   `cargo run` flashes the board with `espflash` and opens the serial monitor.

**The first build is slow.** It downloads ESP-IDF v5.5.1 and its tools into `.embuild/` inside the project and compiles them.

**Rebuilding after configuration changes:** run `cargo clean` before building if you change `sdkconfig.defaults` or the ESP-IDF version (set in `.cargo/config.toml`). Neither change is picked up by an incremental build.

### Configuration

The ESP-IDF settings that matter, in `sdkconfig.defaults`:
- **Flash and PSRAM:** 16 MB flash and 8 MB octal PSRAM at 80 MHz, with PSRAM available to `malloc`.
- **Main task stack:** 16 KB.
- **Wi-Fi and network buffers in PSRAM:** `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP` lets them live there where possible.
- **CPU statistics:** `CONFIG_FREERTOS_GENERATE_RUN_TIME_STATS` lets the status screen show the load on each core.
- **UDP receive queue:** `CONFIG_LWIP_UDP_RECVMBOX_SIZE=32` lengthens it, so bursts of client packets aren't dropped while the server is busy.

## The SD card

Format the card as FAT32 and insert it before powering up. The server creates what it needs:

```
/sd/auth/       accounts (salt and SRP verifier per player)
/sd/world/      the world: nodes.log (every change made) and the generator version
/sd/players/    <name>.inv (inventory) and <name>.player (position, health, skin)
/sd/media/      optional extra media (see below)
/sd/motd.txt    optional welcome message (see below)
/sd/wifi.txt    the Wi-Fi networks to join (see below)
```

All of these are plain text or simple binary files, so you can inspect or edit them on a PC. If a future version changes the terrain generator, the old change log is kept as `nodes-v<n>.log` and the world starts afresh.

### Wi-Fi networks

The server joins Wi-Fi using `/sd/wifi.txt`, a plain text file you can edit on a PC:

```
network My Home Wi-Fi
password correct horse battery staple

network Office
password hunter2
```

Each `network` line starts an entry. Its `password` line is optional; leave it out for an open network. Everything after the first space is the value, spaces included. Lines starting with `#` are ignored.

How the server chooses a network:
- **Which one:** at start-up it scans, then joins the networks it can see, in the file's order. If none are in range it also tries the rest, in case one is hidden, and keeps retrying every 10 seconds. The LCD shows what it is doing.
- **Building with Wi-Fi details:** if you build with `WIFI_SSID` and `WIFI_PASS`, that network is written to the top of `wifi.txt` the first time the new firmware starts, or its password is updated if it is already listed. After that the file is yours: later edits are kept, even through reflashing the same firmware. It is only rewritten if you build with a different network or password.
- **Building without them:** plain `cargo run --release` uses whatever `wifi.txt` holds.

The passwords are stored on the card in plain text, so treat the card accordingly.

### Welcome message

Players see a short welcome in the chat area when they join: what the server is, where the source is, and who else is playing. To use your own text instead, put it in `/sd/motd.txt`; each line becomes one chat line. The list of other players is still added at the end.

### Starting afresh

To wipe the world and start over, build with `RESET_SD=1`:

```
RESET_SD=1 WIFI_SSID='your network' WIFI_PASS='your password' cargo run --release
```

When that firmware starts, it deletes `/sd/auth`, `/sd/world` and `/sd/players`: every account, the world's changes, and all inventories and player files. The world is then generated afresh from the seed. Extra media in `/sd/media` are kept.

It happens once. Each `RESET_SD=1` build has its own token, which it writes to `/sd/reset-done` after the reset, so rebooting the board doesn't wipe the card again. Build without `RESET_SD` afterwards as usual. Alternatively, delete those three folders by hand with the card in a PC.

### Extra media and skins

Textures and models are built into the firmware. A file in `/sd/media` with the same name as a built-in one replaces it, and new files are offered to clients as well.

To add a skin, put `skin_<name>.png` in `/sd/media` and restart. It appears in the skin chooser as "<Name>": for example, `skin_red_knight.png` appears as "Red knight". Both skin formats are accepted:
- **64×32:** the classic Minetest and old Minecraft layout;
- **64×64:** modern Minecraft skins. Luanti's player model has no place for their overlay layers, so only the top half is used.

## Connecting

1. Watch the serial monitor or the LCD for the board's IP address.
2. In Luanti, open **Join Game** and enter that address with port **30000**.
3. Pick a name and password; the first login creates the account.

## Project layout

```
src/main.rs        start-up: SD card, Wi-Fi, LCD, and the memory allocator
src/server.rs      the server loop: peers, reliability, and all game logic
src/protocol.rs    Luanti's UDP packet framing
src/messages.rs    building and parsing protocol commands
src/content.rs     node and item definitions, media
src/world.rs       the world: generated blocks, player changes, lighting
src/terrain.rs     terrain, biomes, caves and ores
src/items.rs       dropped items and their physics
src/inventory.rs   inventories and inventory actions
src/player.rs      saved player state
src/skins.rs       player skins
src/auth.rs        accounts; src/srp.rs, sha256.rs, sha1.rs: login crypto
src/zstd.rs        a small zstd compressor for map blocks
src/lcd.rs, screen.rs, font.rs, status.rs   the status display
src/storage.rs     safe file writes on FAT
media/             built-in textures and models
```

## Notes for hackers

- **Read the engine source.** The protocol has no separate specification. The Luanti source (`src/network/`, `src/server/`, `src/client/`) is the reference, and the comments in this code point to the functions they follow.
- **The Xtensa argument limit.** Functions here are kept to at most six argument words, counting a hidden pointer for large return values. A seventh argument is passed on the stack, and in one build it arrived corrupted, which crashed the server on the first packet. Group arguments into a struct instead; `server::Setup` is the example.
- **Two cores.** The server loop (network and game logic) runs on core 0, next to the Wi-Fi driver. Map blocks, the expensive part, are generated and compressed by a thread on core 1, working from a snapshot of the world's changes. Built blocks are cached, so players near each other share them. A block the world changed around while it was being built is built again.
- **Memory.** The global allocator in `main.rs` puts the server's data in PSRAM. Left to the default, small allocations fill the internal RAM, and then Wi-Fi sends fail and threads can't start.

## Licence

The code is released under the [MIT licence](LICENSE).

The media in `media/` have their own licences:
- **From [Minetest Game](https://github.com/luanti-org/minetest_game), CC BY-SA 3.0:** the player model `character.b3d`, the default skin `character.png`, and the torch models and textures. Their credits are in `media/LICENSE-character.txt` and `media/LICENSE-torch.txt`. If you redistribute these files, the CC BY-SA terms apply to them, whatever you do with the code.
- **Sounds from Minetest Game** (`default_*.ogg`, `player_damage.ogg`): CC BY-SA 3.0, CC BY 3.0 or CC0, with credits in `media/LICENSE-sounds.txt`.
- **The skins `esp_skin_wizard.png` and `esp_skin_elf.png`:** made by Keir Finlow-Bates, licensed under [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).
- **The other `esp_*.png` textures:** made for this project and covered by the MIT licence.

Luanti is a trademark of its respective owners. This project is not affiliated with the Luanti project.
