use std::io::{Read, Write};

use result::{BfResult, Error};

/// The default number of 8-bit memory cells, as used by
/// [`State::new`](struct.State.html#method.new).
pub const DEFAULT_CAPACITY: usize = 30_000;

/// The BF machine state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    memory: Box<[u8]>,
    pointer: usize,
}

impl State {
    /// Creates a new BF machine state with capacity
    /// [`DEFAULT_CAPACITY`].
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Creates a new BF machine state.
    pub fn with_capacity(capacity: usize) -> Self {
        State {
            memory: vec![0; capacity].into_boxed_slice(),
            pointer: 0,
        }
    }

    /// Decrements the pointer.
    ///
    /// # Errors
    ///
    /// Return `Err` if pointer would go below 0.
    #[inline]
    pub fn left(&mut self, count: usize) -> BfResult<()> {
        self.pointer = self.neg_offset(count)?;
        Ok(())
    }

    /// Increments the pointer.
    ///
    /// # Errors
    ///
    /// Return `Err` if pointer would go past the end of the memory.
    #[inline]
    pub fn right(&mut self, count: usize) -> BfResult<()> {
        self.pointer = self.pos_offset(count)?;
        Ok(())
    }

    #[inline]
    fn pos_offset(&self, offset: usize) -> BfResult<usize> {
        if self.pointer + offset < self.memory.len() {
            Ok(self.pointer + offset)
        } else {
            Err(Error::PointerOverflow)
        }
    }

    #[inline]
    fn neg_offset(&self, offset: usize) -> BfResult<usize> {
        if self.pointer >= offset {
            Ok(self.pointer - offset)
        } else {
            Err(Error::PointerUnderflow)
        }
    }

    /// Increments the byte at the pointer.
    ///
    /// Wraps around modulo 256.
    #[inline]
    pub fn up(&mut self, count: u8) {
        let old = self.load();
        self.store(old.wrapping_add(count));
    }

    /// Decrements the byte at the pointer.
    ///
    /// Wraps around modulo 256.
    #[inline]
    pub fn down(&mut self, count: u8) {
        let old = self.load();
        self.store(old.wrapping_sub(count));
    }

    /// Gets the value of the point at the pointer.
    #[inline]
    pub fn load(&self) -> u8 {
        self.memory[self.pointer]
    }

    /// Sets the value of the byte at the pointer.
    #[inline]
    pub fn store(&mut self, value: u8) {
        self.memory[self.pointer] = value;
    }

    /// Adds the given value at the given positive offset from the pointer.
    #[inline]
    pub fn up_pos_offset(&mut self, offset: usize, value: u8) -> BfResult<()> {
        let address = self.pos_offset(offset)?;
        let old = self.memory[address];
        self.memory[address] = old.wrapping_add(value);
        Ok(())
    }

    /// Adds the given value at the given negative offset from the pointer.
    #[inline]
    pub fn up_neg_offset(&mut self, offset: usize, value: u8) -> BfResult<()> {
        let address = self.neg_offset(offset)?;
        let old = self.memory[address];
        self.memory[address] = old.wrapping_add(value);
        Ok(())
    }

    /// Reads from a `Read` into the byte at the pointer.
    #[inline]
    pub fn read<R: Read>(&mut self, input: &mut R) {
        let mut byte = [0];
        let _ = input.read_exact(&mut byte);
        self.store(byte[0]);
    }

    /// Writes to a `Write` from the byte at the pointer.
    #[inline]
    pub fn write<W: Write>(&self, output: &mut W) {
        let _ = output.write_all(&[self.load()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_moves_right() {
        let mut actual = make(&[0, 0, 0], 0);
        let expected = make(&[0, 0, 0], 1);

        actual.right(1).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn right_then_left_restores() {
        let mut actual = make(&[0, 0, 0], 0);
        let expected = make(&[0, 0, 0], 0);

        actual.right(1).unwrap();
        actual.left(1).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn up_goes_to_1() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.up(1);
        assert_eq!(actual, make(&[1, 0, 0], 0))
    }

    #[test]
    fn down_goes_to_255() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.down(1);
        assert_eq!(actual, make(&[255, 0, 0], 0))
    }

    #[test]
    fn load_reads() {
        assert_eq!(make(&[0, 0, 0], 0).load(), 0);
        assert_eq!(make(&[1, 0, 0], 0).load(), 1);
        assert_eq!(make(&[1, 2, 0], 1).load(), 2);
    }

    #[test]
    fn store_writes() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.store(5);
        assert_eq!(actual, make(&[5, 0, 0], 0));
        actual.right(1).unwrap();
        actual.store(8);
        assert_eq!(actual, make(&[5, 8, 0], 1));
    }

    #[test]
    fn longer_sequence_of_actions() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.up(1);
        assert_eq!(actual, make(&[1, 0, 0], 0));
        actual.up(1);
        assert_eq!(actual, make(&[2, 0, 0], 0));
        actual.right(1).unwrap();
        assert_eq!(actual, make(&[2, 0, 0], 1));
        actual.down(1);
        assert_eq!(actual, make(&[2, 255, 0], 1));
        actual.down(1);
        assert_eq!(actual, make(&[2, 254, 0], 1));
        actual.right(1).unwrap();
        assert_eq!(actual, make(&[2, 254, 0], 2));
        actual.store(77);
        assert_eq!(actual, make(&[2, 254, 77], 2));
    }

    #[test]
    fn right_to_right_edge_is_okay() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.right(1).unwrap();
        actual.right(1).unwrap();
        assert_eq!(actual, make(&[0, 0, 0], 2));
    }

    #[test]
    #[should_panic]
    fn right_past_edge_is_error() {
        let mut actual = make(&[0, 0, 0], 0);
        actual.right(1).unwrap();
        actual.right(1).unwrap();
        actual.right(1).unwrap();
    }

    #[test]
    #[should_panic]
    fn move_left_is_error() {
        let mut machine = make(&[0, 0, 0], 0);
        machine.left(1).unwrap();
    }

    fn make(memory: &[u8], pointer: usize) -> State {
        State {
            memory: memory.to_vec().into_boxed_slice(),
            pointer: pointer,
        }
    }
}
