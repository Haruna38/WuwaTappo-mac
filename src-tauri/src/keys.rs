//! Key code tables: macOS virtual key codes ⇄ Windows VK codes (the
//! frontend's shortcut bindings use VK codes) and DOM `KeyboardEvent.key`
//! names (the frontend displays `key-press` payloads as-is).

/// Windows VK code → macOS virtual key code.
pub fn vk_to_mac(vk: u32) -> Option<u16> {
    Some(match vk {
        8 => 51,   // Backspace
        9 => 48,   // Tab
        13 => 36,  // Enter
        27 => 53,  // Escape
        32 => 49,  // Space
        33 => 116, // PageUp
        34 => 121, // PageDown
        35 => 119, // End
        36 => 115, // Home
        37 => 123, // Left
        38 => 126, // Up
        39 => 124, // Right
        40 => 125, // Down
        45 => 114, // Insert (Help)
        46 => 117, // Delete
        48 => 29,
        49 => 18,
        50 => 19,
        51 => 20,
        52 => 21,
        53 => 23,
        54 => 22,
        55 => 26,
        56 => 28,
        57 => 25,
        65 => 0,  // A
        66 => 11, // B
        67 => 8,  // C
        68 => 2,  // D
        69 => 14, // E
        70 => 3,  // F
        71 => 5,  // G
        72 => 4,  // H
        73 => 34, // I
        74 => 38, // J
        75 => 40, // K
        76 => 37, // L
        77 => 46, // M
        78 => 45, // N
        79 => 31, // O
        80 => 35, // P
        81 => 12, // Q
        82 => 15, // R
        83 => 1,  // S
        84 => 17, // T
        85 => 32, // U
        86 => 9,  // V
        87 => 13, // W
        88 => 7,  // X
        89 => 16, // Y
        90 => 6,  // Z
        96 => 82, // Numpad0
        97 => 83,
        98 => 84,
        99 => 85,
        100 => 86,
        101 => 87,
        102 => 88,
        103 => 89,
        104 => 91,
        105 => 92,
        106 => 67, // Numpad *
        107 => 69, // Numpad +
        109 => 78, // Numpad -
        110 => 65, // Numpad .
        111 => 75, // Numpad /
        112 => 122, // F1
        113 => 120,
        114 => 99,
        115 => 118,
        116 => 96,
        117 => 97,
        118 => 98,
        119 => 100,
        120 => 101,
        121 => 109,
        122 => 103,
        123 => 111,
        124 => 105,
        125 => 107,
        126 => 113,
        127 => 106,
        128 => 64,
        129 => 79,
        130 => 80,
        131 => 90, // F20
        186 => 41, // ;
        187 => 24, // =
        188 => 43, // ,
        189 => 27, // -
        190 => 47, // .
        191 => 44, // /
        192 => 50, // `
        219 => 33, // [
        220 => 42, // \
        221 => 30, // ]
        222 => 39, // '
        _ => return None,
    })
}

/// macOS virtual key code → DOM-style key name (layout-independent, US).
pub fn mac_key_name(code: u16) -> String {
    let s: &str = match code {
        0 => "a",
        1 => "s",
        2 => "d",
        3 => "f",
        4 => "h",
        5 => "g",
        6 => "z",
        7 => "x",
        8 => "c",
        9 => "v",
        10 => "§",
        11 => "b",
        12 => "q",
        13 => "w",
        14 => "e",
        15 => "r",
        16 => "y",
        17 => "t",
        18 => "1",
        19 => "2",
        20 => "3",
        21 => "4",
        22 => "6",
        23 => "5",
        24 => "=",
        25 => "9",
        26 => "7",
        27 => "-",
        28 => "8",
        29 => "0",
        30 => "]",
        31 => "o",
        32 => "u",
        33 => "[",
        34 => "i",
        35 => "p",
        36 => "Enter",
        37 => "l",
        38 => "j",
        39 => "'",
        40 => "k",
        41 => ";",
        42 => "\\",
        43 => ",",
        44 => "/",
        45 => "n",
        46 => "m",
        47 => ".",
        48 => "Tab",
        49 => " ",
        50 => "`",
        51 => "Backspace",
        53 => "Escape",
        54 => "Meta",
        55 => "Meta",
        56 => "Shift",
        57 => "CapsLock",
        58 => "Alt",
        59 => "Control",
        60 => "Shift",
        61 => "Alt",
        62 => "Control",
        63 => "Fn",
        64 => "F17",
        65 => ".",
        67 => "*",
        69 => "+",
        71 => "Clear",
        75 => "/",
        76 => "Enter",
        78 => "-",
        79 => "F18",
        80 => "F19",
        81 => "=",
        82 => "0",
        83 => "1",
        84 => "2",
        85 => "3",
        86 => "4",
        87 => "5",
        88 => "6",
        89 => "7",
        90 => "F20",
        91 => "8",
        92 => "9",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        99 => "F3",
        100 => "F8",
        101 => "F9",
        103 => "F11",
        105 => "F13",
        106 => "F16",
        107 => "F14",
        109 => "F10",
        111 => "F12",
        113 => "F15",
        114 => "Insert",
        115 => "Home",
        116 => "PageUp",
        117 => "Delete",
        118 => "F4",
        119 => "End",
        120 => "F2",
        121 => "PageDown",
        122 => "F1",
        123 => "ArrowLeft",
        124 => "ArrowRight",
        125 => "ArrowDown",
        126 => "ArrowUp",
        _ => return format!("Key{code}"),
    };
    s.to_string()
}

/// CGEventFlags bits → frontend modifier bitmask (Alt=1, Ctrl=2, Shift=4, Win/Cmd=8).
pub fn flags_to_modifiers(flags: u64) -> u32 {
    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const OPTION: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;
    let mut m = 0;
    if flags & OPTION != 0 {
        m |= 1;
    }
    if flags & CONTROL != 0 {
        m |= 2;
    }
    if flags & SHIFT != 0 {
        m |= 4;
    }
    if flags & COMMAND != 0 {
        m |= 8;
    }
    m
}

/// Modifier flag bits we report as key-press/key-release on FlagsChanged.
pub const MODIFIER_FLAG_NAMES: &[(u64, &str)] = &[
    (0x0001_0000, "CapsLock"),
    (0x0002_0000, "Shift"),
    (0x0004_0000, "Control"),
    (0x0008_0000, "Alt"),
    (0x0010_0000, "Meta"),
    (0x0080_0000, "Fn"),
];
