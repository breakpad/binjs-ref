use std;
use std::io::{Read, Write};

pub trait WriteVarNum {
    fn write_varnum(&mut self, num: u32) -> Result<usize, std::io::Error>;
}

pub trait ReadVarNum {
    fn read_varnum(&mut self, num: &mut u32) -> Result<usize, std::io::Error>;
}

impl<T> WriteVarNum for T where T: Write {
    fn write_varnum(&mut self, mut value: u32) -> Result<usize, std::io::Error> {
        let mut bytes = Vec::with_capacity(4);
        loop {
            let mut byte = ((value & 0x7F) << 1) as u8;
            if value > 0x7F {
                byte |= 1;
            }
            bytes.push(byte);
            value >>= 7;
            if value == 0 {
                break
            }
        }
        self.write(&bytes)
    }
}

impl<T> ReadVarNum for T where T: Read {
    fn read_varnum(&mut self, num: &mut u32) -> Result<usize, std::io::Error> {
        let mut bytes = 0;
        let mut result : u32 = 0;
        let mut shift : u32 = 0;
        let mut buf : [u8;1] = [0];
        loop {
            debug_assert!(shift < 32);
            bytes += self.read(&mut buf)?;

            let byte = buf[0];
            result |= (byte as u32 >> 1) << shift;
            shift += 7;
            if byte & 1 == 0 {
                *num = result;
                return Ok(bytes);
            }
        }
    }
}

#[test]
fn test_varnum() {
    use std::io::Cursor;
    // Produce a reasonably unbiaised sample of numbers.
    for i in 1..5 {
        let mut start = i;
        for num in &[3, 5, 7, 11, 13] {
            start *= *num;

            println!("test_varnum, testing with {}", start);
            let mut encoded = vec![];
            let encoded_bytes = encoded.write_varnum(start).unwrap();
            assert_eq!(encoded_bytes, encoded.len());
            println!("test_varnum, encoded as {:?}", encoded);

            let mut decoded : u32 = 0;
            let decoded_bytes = Cursor::new(encoded).read_varnum(&mut decoded).unwrap();

            assert_eq!(start, decoded);
            assert_eq!(encoded_bytes, decoded_bytes);
        }
    }
}