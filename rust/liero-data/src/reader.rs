use crate::Error;

/// Little-endian cursor over the bytes of an input file.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    name: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], name: &'static str) -> Self {
        Reader { data, pos: 0, name }
    }

    fn eof(&self) -> Error {
        Error(format!("{} ends early (at offset {})", self.name, self.pos))
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), Error> {
        if pos > self.data.len() {
            return Err(Error(format!("{} is too short (wanted offset {})", self.name, pos)));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<(), Error> {
        self.seek(self.pos + n)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| self.eof())?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, Error> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, Error> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Pascal string: a length byte followed by that many bytes.
    pub fn pascal_string(&mut self) -> Result<Vec<u8>, Error> {
        let len = self.u8()? as usize;
        Ok(self.bytes(len)?.to_vec())
    }

    /// Pascal string stored in a fixed-size field of `field_len` bytes.
    pub fn pascal_string_field(&mut self, field_len: usize) -> Result<Vec<u8>, Error> {
        let field = self.bytes(field_len)?;
        let len = (field[0] as usize).min(field_len - 1);
        Ok(field[1..1 + len].to_vec())
    }
}
