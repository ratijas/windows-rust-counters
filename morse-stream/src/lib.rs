//! # Morse Code encoder & decoder
//!
//! Supports International (ITU) dialect.
#![deny(dead_code)]

use std::error::Error;
use std::fmt::{self, Debug};
use std::num::NonZeroU8;

use signal_flow::*;

use self::CodePoint::*;

pub type Signal = bool;

pub const ON: Signal = true;
pub const OFF: Signal = false;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CodePoint {
    /// The dot duration is the basic unit of time measurement in Morse code transmission.
    Dot,
    /// The duration of a dash is three times the duration of a dot.
    Dash,
    /// Spacing between `Dot`s and `Dash`es, equal to the dot duration.
    SymbolPause,
    /// The letters of a word are separated by a space of duration equal to three dots.
    LetterPause,
    /// The words are separated by a space equal to seven dots.
    WordPause,
}

// Shortcuts for visibility.  Width is a multiple of two because comma and the following whitespace
// separating them are two characters.
pub const II: CodePoint = Dot;
pub const OOOOOO: CodePoint = Dash;

impl CodePoint {
    pub fn duration(self) -> u8 {
        match self {
            Dot | SymbolPause => 1,
            Dash | LetterPause => 3,
            WordPause => 7,
        }
    }

    pub fn encode(self) -> &'static [Signal] {
        match self {
            Dot => &[ON],
            Dash => &[ON; 3],
            SymbolPause => &[OFF],
            LetterPause => &[OFF; 3],
            WordPause => &[OFF; 7],
        }
    }

    pub fn is_dot_or_dash(self) -> bool {
        match self {
            Dot | Dash => true,
            _ => false,
        }
    }

    pub fn is_space(self) -> bool {
        match self {
            SymbolPause | LetterPause | WordPause => true,
            _ => false,
        }
    }
}

/// Invariant: `KnownCodePoints` contain only `Dot` and `Dash` `CodePoint` items.
pub type KnownCodePoints = &'static [CodePoint];
pub type Table = &'static [(char, KnownCodePoints)];

pub trait Dialect: Clone + Debug + Default {
    /// Provide encoding/decoding table for all symbols known to the dialect.
    fn table(&self) -> Table;

    fn can_encode(&self, char: char) -> bool {
        char.is_ascii_whitespace() || self.encode_char(char).is_some()
    }

    /// Provide encoding for given letter, if any. Spaces should not be recognized, and must be dealt with elsewhere.
    fn encode_char(&self, char: char) -> Option<KnownCodePoints> {
        let upper = char.to_uppercase().next().unwrap();
        self.table().iter()
            .find(|(ch, _)| *ch == upper || *ch == char)
            .map(|(_, code)| *code)
    }

    fn encode_unknown(&self) -> KnownCodePoints;

    /// In order to recognize a letter, it must be followed by a letter space (silence for as long as three dots.
    /// Code points are guaranteed to only contain dots and dashes. Spaces must be dealt with elsewhere. This function assumes that given sequence is a complete encoded letter. Sequence must be non-empty.
    fn decode_char(&self, seq: &[CodePoint]) -> Option<char> {
        assert!(!seq.is_empty());
        assert!(seq.iter().all(|&code| code == Dot || code == Dash));

        self.table().iter()
            .find(|(_, encoding)| *encoding == seq)
            .map(|(char, _)| *char)
    }

    fn encoder<X: Tx<Item=Signal>>(tx: X) -> EncoderTx<Self, X> {
        EncoderTx::new(tx)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ITU;

impl ITU {
    const TABLE_ITU: Table = &[
        // Letters
        ('A', &[II, OOOOOO]),
        ('B', &[OOOOOO, II, II, II]),
        ('C', &[OOOOOO, II, OOOOOO, II]),
        ('D', &[OOOOOO, II, II]),
        ('E', &[II]),
        ('F', &[II, II, OOOOOO, II]),
        ('G', &[OOOOOO, OOOOOO, II]),
        ('H', &[II, II, II, II]),
        ('I', &[II, II]),
        ('J', &[II, OOOOOO, OOOOOO, OOOOOO]),
        ('K', &[OOOOOO, II, OOOOOO]),
        ('L', &[II, OOOOOO, II, II]),
        ('M', &[OOOOOO, OOOOOO]),
        ('N', &[OOOOOO, II]),
        ('O', &[OOOOOO, OOOOOO, OOOOOO]),
        ('P', &[II, OOOOOO, OOOOOO, II]),
        ('Q', &[OOOOOO, OOOOOO, II, OOOOOO]),
        ('R', &[II, OOOOOO, II]),
        ('S', &[II, II, II]),
        ('T', &[OOOOOO]),
        ('U', &[II, II, OOOOOO]),
        ('V', &[II, II, II, OOOOOO]),
        ('W', &[II, OOOOOO, OOOOOO]),
        ('X', &[OOOOOO, II, II, OOOOOO]),
        ('Y', &[OOOOOO, II, OOOOOO, OOOOOO]),
        ('Z', &[OOOOOO, OOOOOO, II, II]),
        // Numbers
        ('1', &[II, OOOOOO, OOOOOO, OOOOOO, OOOOOO]),
        ('2', &[II, II, OOOOOO, OOOOOO, OOOOOO]),
        ('3', &[II, II, II, OOOOOO, OOOOOO]),
        ('4', &[II, II, II, II, OOOOOO]),
        ('5', &[II, II, II, II, II]),
        ('6', &[OOOOOO, II, II, II, II]),
        ('7', &[OOOOOO, OOOOOO, II, II, II]),
        ('8', &[OOOOOO, OOOOOO, OOOOOO, II, II]),
        ('9', &[OOOOOO, OOOOOO, OOOOOO, OOOOOO, II]),
        ('0', &[OOOOOO, OOOOOO, OOOOOO, OOOOOO, OOOOOO]),
        ('0', &[OOOOOO]), // 0 (alt)
        // Punctuation
        ('.', &[II, OOOOOO, II, OOOOOO, II, OOOOOO]),      // Period [.]
        (',', &[OOOOOO, OOOOOO, II, II, OOOOOO, OOOOOO]),  // Comma [,]
        ('?', &[II, II, OOOOOO, OOOOOO, II, II]),          // Question Mark [?]
        ('\'', &[II, OOOOOO, OOOOOO, OOOOOO, OOOOOO, II]), // Apostrophe [']
        ('!', &[OOOOOO, II, OOOOOO, II, OOOOOO, OOOOOO]),  // Exclamation Point [!]; [KW] digraph
        ('/', &[OOOOOO, II, II, OOOOOO, II]),              // Slash/Fraction Bar [/]
        ('(', &[OOOOOO, II, OOOOOO, OOOOOO, II]),          // Parenthesis (Open)
        (')', &[OOOOOO, II, OOOOOO, OOOOOO, II, OOOOOO]),  // Parenthesis (Close)
        ('&', &[II, OOOOOO, II, II, II]),                  // Ampersand (or "Wait") [&]; [AS] digraph
        (':', &[OOOOOO, OOOOOO, OOOOOO, II, II, II]),      // Colon [:]
        ('=', &[OOOOOO, II, II, II, OOOOOO]),              // Double Dash [=]
        ('+', &[II, OOOOOO, II, OOOOOO, II]),              // Plus sign [+]
        ('-', &[OOOOOO, II, II, II, II, OOOOOO]),          // Hyphen, Minus Sign [-]
        ('"', &[II, OOOOOO, II, II, OOOOOO, II]),          // Quotation mark ["]
        ('@', &[II, OOOOOO, OOOOOO, II, OOOOOO, II]),      // At Sign [@]; [AC] digraph
    ];
    const EMPTY: KnownCodePoints = &[];
}

impl Dialect for ITU {
    fn table(&self) -> Table {
        Self::TABLE_ITU
    }

    fn encode_unknown(&self) -> KnownCodePoints {
        Self::EMPTY
    }
}


///////////////////////////////////////////////
/////////////////// Encoder ///////////////////
///////////////////////////////////////////////


pub struct EncoderTx<D, X> {
    dialect: D,
    tx: X,
    /// How long is the current pause?
    pause_duration: u8,
    /// How much of the pause is already written?
    pause_written: u8,
}

impl<D: Dialect, X: Tx<Item=Signal>> EncoderTx<D, X> {
    pub fn new(tx: X) -> Self {
        EncoderTx {
            dialect: Default::default(),
            tx,
            pause_duration: 0,
            pause_written: 0,
        }
    }

    fn set_pause(&mut self, pause: CodePoint) -> Result<(), Box<dyn Error>> {
        assert!(pause.is_space());

        self.pause_duration = self.pause_duration.max(pause.duration());
        self.send_pause()?;

        Ok(())
    }

    fn flush_pause(&mut self) -> Result<(), Box<dyn Error>> {
        self.send_pause()?;

        self.pause_duration = 0;
        self.pause_written = 0;

        Ok(())
    }

    fn send_pause(&mut self) -> Result<(), Box<dyn Error>> {
        assert!(self.pause_written <= self.pause_duration);

        for _ in 0..(self.pause_duration - self.pause_written) {
            self.tx.send(OFF)?;
        }

        self.pause_written = self.pause_duration;

        Ok(())
    }

    fn send_dot_or_dash(&mut self, symbol: CodePoint) -> Result<(), Box<dyn Error>> {
        assert!(symbol.is_dot_or_dash());

        self.flush_pause()?;
        self.tx.send_all(symbol.encode().iter().cloned())?;
        self.set_pause(SymbolPause)?;

        Ok(())
    }

    fn send_encoded_char(&mut self, symbols: KnownCodePoints) -> Result<(), Box<dyn Error>> {
        assert!(symbols.iter().all(|symbol| symbol.is_dot_or_dash()));

        for code in symbols {
            self.send_dot_or_dash(*code)?;
        }
        self.set_pause(LetterPause)?;

        Ok(())
    }

    fn send_unknown(&mut self) -> Result<(), Box<dyn Error>> {
        self.flush_pause()?;
        self.tx.send_all(
            self.dialect
                .encode_unknown()
                .iter()
                .cloned()
                .flat_map(CodePoint::encode)
                .cloned()
        )?;

        Ok(())
    }

    fn send_char(&mut self, char: char) -> Result<(), Box<dyn Error>> {
        if let Some(symbols) = self.dialect.encode_char(char) {
            // handle known char
            self.send_encoded_char(symbols)?;
        } else if char.is_ascii_whitespace() {
            // handle whitespace
            self.set_pause(WordPause)?;
        } else {
            // handle something that Morse cannot handle
            self.send_unknown()?;
        }

        Ok(())
    }
}

impl<D: Dialect> EncoderTx<D, ()> {
    /// Encode string with Morse code, producing a sequence of on/off signal states.
    pub fn encode_str<S: AsRef<str>>(s: S) -> Vec<Signal> {
        let mut buffer = Vec::new();

        let mut coder = EncoderTx::<D, _>::new(VecCollectorTx::new(&mut buffer));
        for char in s.as_ref().chars() {
            coder.send_char(char).unwrap();
        }
        // unlock buffer from borrow checker
        drop(coder);
        buffer
    }
}

impl<D: Dialect, X: Tx<Item=Signal>> Tx for EncoderTx<D, X> {
    type Item = char;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        self.send_char(value)
    }
}


///////////////////////////////////////////////
///////////////////  Error  ///////////////////
///////////////////////////////////////////////

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MorseDecodeError {
    /// Failed to decode raw 0s and 1s into dots and dashes.
    Signal {
        signal: Vec<Signal>,
    },
    /// Failed to decode dots and dashed as a letter.
    Letter {
        code_points: Vec<CodePoint>,
    },
}

impl MorseDecodeError {
    pub fn from_signal(signal: Vec<Signal>) -> Self {
        MorseDecodeError::Signal {
            signal
        }
    }
    pub fn from_letter(code_points: Vec<CodePoint>) -> Self {
        MorseDecodeError::Letter {
            code_points
        }
    }
}

impl Error for MorseDecodeError {}

impl fmt::Display for MorseDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}


///////////////////////////////////////////////
/////////////////// Decoder ///////////////////
///////////////////////////////////////////////


/// Current value of the signal and duration for how long the signal has been maintaining this
/// same value.
#[derive(Clone, Copy, Debug)]
pub struct SignalGroup {
    pub state: Signal,
    /// Duration larger than 3 units of `ON` state is an unconditional error.
    /// Duration larger than 7 units of `OFF` state is considered as a single word space, i.e.
    /// all `OFF`s after 7th are ignored.
    pub duration: NonZeroU8,
}

impl SignalGroup {
    pub fn new(state: Signal) -> Self {
        SignalGroup {
            state,
            // SAFETY: constant 1 is obviously not 0
            duration: unsafe { NonZeroU8::new_unchecked(1) },
        }
    }

    /// Increment duration of this state.
    ///
    /// Uses saturating addition because values that are big enough to overflow are not meaningful
    /// to morse code anyway.
    pub fn inc(&mut self) {
        // SAFETY: for any duration (u8): saturating addition of constant 1 can not result in 0
        self.duration = unsafe { NonZeroU8::new_unchecked(self.duration.get().saturating_add(1)) };
    }

    /// Validate group:
    ///   - For `ON` state, duration must be between 1 and 3 inclusive.
    ///   - For `OFF` state, duration can be any (non-zero value).
    pub fn is_valid(&self) -> bool {
        if self.state == ON {
            self.duration.get() <= 3
        } else {
            true
        }
    }

    /// Compare whether the `state` is not different from this group's one.
    pub fn is_same(&self, state: Signal) -> bool {
        self.state == state
    }

    /// Map state and its duration to the exact matching CodePoint.
    /// Rounds **DOWN** the duration of `OFF` states to the nearest meaningful value:
    ///   - 7 for pause after word
    ///   - 3 for pause after letter
    ///   - 1 for pause after symbol
    pub fn to_code_point(&self) -> Option<CodePoint> {
        match self.state {
            ON => match self.duration.get() {
                1 => Some(Dot),
                3 => Some(Dash),
                _ => None
            },
            OFF => match self.duration.get() {
                w if w >= 7 => Some(WordPause),
                l if l >= 3 && l < 7 => Some(LetterPause),
                s if s >= 1 && s < 3 => Some(SymbolPause),
                _ => None
            }
        }
    }
}

pub struct DecoderRx<D, X> {
    dialect: D,
    inner: X,
    /// Current state of parser can be derived from the `current_group` value.
    ///
    /// Signal group contains last seen signal unit, and duration for how long the signal has been
    /// emitting the same unit value.
    ///
    /// `current_group` is `None` before the start or after a reset (which is always done after any
    /// detected error).
    current_group: Option<SignalGroup>,
    /// Dots and dashes of current letter.
    current_letter: Vec<CodePoint>,
}

impl<D, X> DecoderRx<D, X>
    where D: Dialect, X: Rx<Item=Signal>
{
    pub fn new(inner: X) -> Self {
        DecoderRx {
            inner,
            dialect: Default::default(),
            // current_state: None,
            // current_duration: 0,
            current_group: None,
            current_letter: Vec::with_capacity(8),
        }
    }

    fn reset_with_signal_error(&mut self) -> Box<dyn Error> {
        self.reset_letter();
        Self::boxed_error_from_group(self.reset_group())
    }

    fn reset_with_letter_error(&mut self) -> Box<dyn Error> {
        let letter = self.current_letter.clone();
        self.reset_letter();
        Box::new(MorseDecodeError::from_letter(letter))
    }

    fn boxed_error_from_group(group: Option<SignalGroup>) -> Box<dyn Error> {
        let signal = match group {
            Some(SignalGroup { state, duration }) => vec![state; duration.get() as usize],
            None => vec![]
        };

        Box::new(MorseDecodeError::from_signal(signal))
    }

    /// Clear `current_group`, returning its old value.
    fn reset_group(&mut self) -> Option<SignalGroup> {
        self.current_group.take()
    }

    /// Clear `current_letter`.
    fn reset_letter(&mut self) {
        self.current_letter.clear();
    }

    /// Read one signal unit from the signal source and return finished group (if any).
    ///
    /// Algorithm:
    ///  - Read one signal util from the signal source;
    ///  - If signal if exhausted:
    ///     * Reset current group to None, returning its old value.
    ///  - Else If signal value changed:
    ///     * Save current group
    ///     * Reset current group to new signal value
    ///     * Return saved value of current group
    ///  - Else:
    ///     * Update current group.
    ///     * Validate current group:
    ///         - For `ON` state, duration must be between 1 and 3 inclusive.
    ///         - For `OFF` state, duration can be any (non-zero value).
    ///     * Return None
    fn read_signal_unit_and_update_current_group(&mut self) -> Result<Option<SignalGroup>, Box<dyn Error>> {
        match self.inner.recv()? {
            None => return Ok(self.reset_group()),
            Some(signal) => self.add_signal_unit(signal)
        }
    }

    /// When this function resets the current group (because signal changed its value), it returns its old value.
    fn add_signal_unit(&mut self, signal: Signal) -> Result<Option<SignalGroup>, Box<dyn Error>> {
        Ok(match self.current_group {
            None => {
                // New signal. Create new group, returning None.
                self.current_group = Some(SignalGroup::new(signal));
                None
            }
            Some(ref mut current) if current.is_same(signal) => {
                // Signal stays same. Increment current group, returning None.
                current.inc();
                if !current.is_valid() {
                    return Err(self.reset_with_signal_error());
                }
                None
            }
            Some(old_group) => {
                // Signal changed. Create new group, returning old one.
                self.current_group = Some(SignalGroup::new(signal));
                Some(old_group)
            }
        })
    }

    fn add_symbol_to_letter(&mut self, symbol: CodePoint) {
        assert!(symbol.is_dot_or_dash());
        self.current_letter.push(symbol);
    }

    fn decode_current_letter(&mut self) -> Result<char, Box<dyn Error>> {
        match self.dialect.decode_char(&self.current_letter) {
            None => Err(self.reset_with_letter_error()),
            Some(char) => {
                self.reset_letter();
                Ok(char)
            }
        }
    }

    fn read_char(&mut self) -> Result<Option<char>, Box<dyn Error>> {
        loop {
            if let Some(group) = self.read_signal_unit_and_update_current_group()? {
                if group.state == ON {
                    let code_point = group.to_code_point()
                        .ok_or_else(|| Self::boxed_error_from_group(Some(group)))?;

                    self.add_symbol_to_letter(code_point);
                    // TODO: check for too long letter error
                } else {
                    // do nothing because we have already dealt with OFF group below.
                }
            } else {
                // Use current group instead of finished one for processing OFFs.
                match self.current_group {
                    Some(SignalGroup { state: OFF, duration }) => {
                        if duration.get() == 7 {
                            // emit whitespace
                            return Ok(Some(' '));
                        } else if duration.get() == 3 {
                            return self.decode_current_letter().map(Some);
                        } else if duration.get() == 1 {
                            // do nothing because dot/dash group is already converted to symbol and added to the current letter
                            // while processing finished ON group above.
                        }
                    }
                    None => {
                        // no finished group AND current group is empty
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }
}

impl<D: Dialect, X: Rx<Item=Signal>> Rx for DecoderRx<D, X> {
    type Item = char;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        self.read_char()
    }
}


///////////////////////////////////////////////
////////////////// Tx/Rx Ext //////////////////
///////////////////////////////////////////////

pub trait MorseTxExt: Tx<Item=Signal> {
    fn morse_encode<D: Dialect>(self) -> EncoderTx<D, Self>
        where Self: Sized
    {
        EncoderTx::new(self)
    }
}

impl<X: Tx<Item=Signal>> MorseTxExt for X {}

pub trait MorseRxExt {
    fn morse_decode<D: Dialect>(self) -> DecoderRx<D, Self>
        where Self: Rx<Item=Signal> + Sized,
              DecoderRx<D, Self>: Sized
    {
        DecoderRx::<D, Self>::new(self)
    }
}

impl<X: Rx> MorseRxExt for X {}


pub fn print<I: IntoIterator<Item=Signal>>(sequence: I) {
    for c in sequence.into_iter() {
        print!("{}", if c { '*' } else { ' ' });
    }
    println!();
}

#[cfg(test)]
mod test {
    use super::*;

    const SOS: &'static [Signal] = &[
        ON, OFF, ON, OFF, ON, // S: · · ·
        OFF, OFF, OFF, // letter space
        ON, ON, ON, OFF, ON, ON, ON, OFF, ON, ON, ON, // O: − − −
        OFF, OFF, OFF, // letter space
        ON, OFF, ON, OFF, ON, // S: · · ·
        OFF, OFF, OFF, // end of message
    ];


    const A_B: &'static [Signal] = &[
        ON, OFF, ON, ON, ON, // A: · −
        OFF, OFF, OFF, OFF, OFF, OFF, OFF, // word space
        ON, ON, ON, OFF, ON, OFF, ON, OFF, ON, // B: − · · ·
        OFF, OFF, OFF, // end of message
    ];

    #[test]
    fn test_encode_simple() {
        let mut buffer = Vec::new();
        ITU::encoder(VecCollectorTx::new(&mut buffer));

        assert_eq!(EncoderTx::<ITU, _>::encode_str("SOS"), vec![
            ON, OFF, ON, OFF, ON, // S: · · ·
            OFF, OFF, OFF, // letter space
            ON, ON, ON, OFF, ON, ON, ON, OFF, ON, ON, ON, // O: − − −
            OFF, OFF, OFF, // letter space
            ON, OFF, ON, OFF, ON, // S: · · ·
            OFF, OFF, OFF, // end of message
        ]);
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(EncoderTx::<ITU, _>::encode_str("a b"), A_B.to_owned());
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_decode_SOS() {
        let coder = DecoderRx::<ITU, _>::new(IteratorRx::from(SOS.to_owned()));
        let decoded: String = coder.collect().unwrap();

        assert_eq!(decoded, "SOS");
    }

    #[test]
    fn test_decode_words() {
        let coder = DecoderRx::<ITU, _>::new(IteratorRx::from(A_B.to_owned()));
        let decoded: String = coder.collect().unwrap();

        assert_eq!(decoded, "A B");
    }

    #[test]
    fn test_decode_early_error() {
        let signal = vec![ON, ON];
        let mut coder = DecoderRx::<ITU, _>::new(IteratorRx::from(signal.clone()));
        let result = coder.recv();
        assert!(result.is_err());
        let err = *(result.err().unwrap().downcast::<MorseDecodeError>().unwrap());
        assert_eq!(err, MorseDecodeError::from_signal(signal));
    }
}
