// Copyright 2026 Renato Cavalcanti
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Finds the bell and the notification escapes in terminal output.

const BEL: u8 = 0x07;
const ESC: u8 = 0x1b;

/// The most bytes kept from the start of each OSC string.
const KEEP: usize = 16;

/// Where the scanner stands in the byte stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Text,
    Escape,
    Osc,
    OscEscape,
}

/// Scans terminal output for a bell or a notification, across chunks.
#[derive(Debug, Default)]
pub struct Scanner {
    mode: Mode,
    osc: Vec<u8>,
}

impl Scanner {
    /// Whether `bytes` holds a bell or a notification escape.
    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        let mut signal = false;

        for &byte in bytes {
            signal |= self.step(byte);
        }

        signal
    }

    /// Moves past `byte`, and tells whether it completes a signal.
    fn step(&mut self, byte: u8) -> bool {
        match (self.mode, byte) {
            (Mode::Text, BEL) => true,
            (Mode::Text, ESC) => {
                self.mode = Mode::Escape;
                false
            }
            (Mode::Text, _) => false,

            (Mode::Escape, b']') => {
                self.osc.clear();
                self.mode = Mode::Osc;
                false
            }
            (Mode::Escape, _) => {
                self.mode = Mode::Text;
                self.step(byte)
            }

            (Mode::Osc, BEL) => {
                self.mode = Mode::Text;
                self.judge()
            }
            (Mode::Osc, ESC) => {
                self.mode = Mode::OscEscape;
                false
            }
            (Mode::Osc, _) => {
                if self.osc.len() < KEEP {
                    self.osc.push(byte);
                }
                false
            }

            (Mode::OscEscape, b'\\') => {
                self.mode = Mode::Text;
                self.judge()
            }
            (Mode::OscEscape, _) => {
                let signal = self.judge();
                self.mode = Mode::Escape;
                self.step(byte) || signal
            }
        }
    }

    /// Whether the kept OSC string is a notification.
    fn judge(&self) -> bool {
        let (number, payload) = match self.osc.iter().position(|&byte| byte == b';') {
            Some(at) => (&self.osc[..at], &self.osc[at + 1..]),
            None => (&self.osc[..], &[][..]),
        };

        match number {
            b"9" => !starts_with_a_number(payload),
            b"777" => payload.starts_with(b"notify;"),
            b"99" => true,
            _ => false,
        }
    }
}

/// Whether `payload` starts with digits followed by `;`.
fn starts_with_a_number(payload: &[u8]) -> bool {
    let digits = payload
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits > 0 && payload.get(digits) == Some(&b';')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bell_is_a_signal() {
        assert!(Scanner::default().feed(b"done\x07"));
    }

    #[test]
    fn plain_output_is_no_signal() {
        assert!(!Scanner::default().feed(b"hello\r\n"));
    }

    #[test]
    fn a_bell_that_ends_an_osc_is_no_bell() {
        assert!(!Scanner::default().feed(b"\x1b]0;title\x07"));
    }

    #[test]
    fn osc_9_text_is_a_signal() {
        assert!(Scanner::default().feed(b"\x1b]9;Claude needs you\x07"));
    }

    #[test]
    fn osc_9_progress_is_no_signal() {
        assert!(!Scanner::default().feed(b"\x1b]9;4;1;50\x07"));
    }

    #[test]
    fn osc_777_notify_is_a_signal() {
        assert!(Scanner::default().feed(b"\x1b]777;notify;Codex;done\x1b\\"));
        assert!(!Scanner::default().feed(b"\x1b]777;other;x\x07"));
    }

    #[test]
    fn osc_99_is_a_signal() {
        assert!(Scanner::default().feed(b"\x1b]99;;hi\x1b\\"));
    }

    #[test]
    fn other_oscs_are_no_signal() {
        let oscs: [&[u8]; 5] = [
            b"\x1b]0;t\x07",
            b"\x1b]2;t\x07",
            b"\x1b]7;file:///a\x07",
            b"\x1b]8;;https://x\x1b\\",
            b"\x1b]133;A\x07",
        ];

        for osc in oscs {
            assert!(
                !Scanner::default().feed(osc),
                "{}",
                String::from_utf8_lossy(osc)
            );
        }
    }

    #[test]
    fn a_signal_split_across_chunks_counts() {
        let mut scanner = Scanner::default();

        assert!(!scanner.feed(b"\x1b]9;wa"));
        assert!(scanner.feed(b"iting\x07"));
    }

    #[test]
    fn an_st_split_across_chunks_ends_the_osc() {
        let mut scanner = Scanner::default();

        assert!(!scanner.feed(b"\x1b]0;t\x1b"));
        assert!(!scanner.feed(b"\\"));
        assert!(scanner.feed(b"\x07"));
    }

    #[test]
    fn a_huge_osc_keeps_little_and_a_later_bell_counts() {
        let mut scanner = Scanner::default();
        let mut osc = b"\x1b]52;c;".to_vec();
        osc.extend(vec![b'A'; 100_000]);
        osc.push(BEL);

        assert!(!scanner.feed(&osc));
        assert!(scanner.osc.len() <= KEEP);
        assert!(scanner.feed(b"\x07"));
    }
}
