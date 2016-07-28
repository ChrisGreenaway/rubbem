extern crate crypto;

use byteorder::{BigEndian,ReadBytesExt};
use self::crypto::digest::Digest;
use self::crypto::sha2::Sha512;
use std::io::Cursor;

pub fn sha512_hash(input: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();

    hasher.input(input);

    let mut result: [u8; 64] = [0; 64];
    hasher.result(&mut result[..]);

    result
}

pub fn sha512_checksum(input: &[u8]) -> u32 {
    let hash = sha512_hash(input);

    assert!(hash.len() >= 4);

    let mut cursor = Cursor::new(&hash[0..4]);
    cursor.read_u32::<BigEndian>().unwrap()
}

pub fn double_sha512_checksum_bytes(input: &[u8]) -> [u8; 4] {
    let hash1 = sha512_hash(input);
    let hash2 = sha512_hash(&hash1[..]);

    let mut result: [u8; 4]  = [0; 4];
    result.clone_from_slice(&hash2[0..4]);

    result
}

#[cfg(test)]
mod tests {
    use super::sha512_checksum;
    use super::sha512_hash;

    #[test]
    fn test_sha512_hash() {
        let input: Vec<u8> = vec![ 104, 101, 108, 108, 111 ]; // hello
        let output1 = sha512_hash(&input[..]);
        let output2 = sha512_hash(&output1[..]);

        let expected1 = [
                        0x9b, 0x71, 0xd2, 0x24, 0xbd, 0x62, 0xf3, 0x78,
                        0x5d, 0x96, 0xd4, 0x6a, 0xd3, 0xea, 0x3d, 0x73,
                        0x31, 0x9b, 0xfb, 0xc2, 0x89, 0x0c, 0xaa, 0xda,
                        0xe2, 0xdf, 0xf7, 0x25, 0x19, 0x67, 0x3c, 0xa7,
                        0x23, 0x23, 0xc3, 0xd9, 0x9b, 0xa5, 0xc1, 0x1d,
                        0x7c, 0x7a, 0xcc, 0x6e, 0x14, 0xb8, 0xc5, 0xda,
                        0x0c, 0x46, 0x63, 0x47, 0x5c, 0x2e, 0x5c, 0x3a,
                        0xde, 0xf4, 0x6f, 0x73, 0xbc, 0xde, 0xc0, 0x43 ];

        assert_eq!(&expected1[..], &output1[..]);

        let expected2 = [
                        0x05, 0x92, 0xa1, 0x05, 0x84, 0xff, 0xab, 0xf9,
                        0x65, 0x39, 0xf3, 0xd7, 0x80, 0xd7, 0x76, 0x82,
                        0x8c, 0x67, 0xda, 0x1a, 0xb5, 0xb1, 0x69, 0xe9,
                        0xe8, 0xae, 0xd8, 0x38, 0xaa, 0xec, 0xc9, 0xed,
                        0x36, 0xd4, 0x9f, 0xf1, 0x42, 0x3c, 0x55, 0xf0,
                        0x19, 0xe0, 0x50, 0xc6, 0x6c, 0x63, 0x24, 0xf5,
                        0x35, 0x88, 0xbe, 0x88, 0x89, 0x4f, 0xef, 0x4d,
                        0xcf, 0xfd, 0xb7, 0x4b, 0x98, 0xe2, 0xb2, 0x00 ];

        assert_eq!(&expected2[..], &output2[..]);
    }

    #[test]
    fn test_sha512_checksum() {
        let bytes = vec![];
        let checksum = sha512_checksum(&bytes[..]);
        assert_eq!(3481526581, checksum);
    }
}
