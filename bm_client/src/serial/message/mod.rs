pub mod read;
pub mod write;

pub use self::read::read_message;
pub use self::write::{write_message,write_object_message_data};

use serial::basic::BasicSerialError;

const MAGIC: u32 = 0xe9beb4d9;
const MAX_PAYLOAD_LENGTH: u32 = 1600003;
const MAX_NODES_COUNT: usize = 1000;
const MAX_GETDATA_COUNT: usize = 50000;
const MAX_INV_COUNT: usize = 50000;
const MAX_PAYLOAD_LENGTH_FOR_OBJECT: u32 = 262144; // 2^18 - maximum object length

#[derive(Debug,PartialEq)]
pub enum MessageSerialError {
    OutOfData,
    BadAscii,
    MaximumValueExceeded,
    BadMagic,
    PayloadSize,
    ChecksumMismatch,
    NonZeroPadding,
    UnknownCommand,
    UnknownObjectType,
    UnknownObjectVersion,
}

impl From<BasicSerialError> for MessageSerialError {
    fn from(error: BasicSerialError) -> MessageSerialError {
        match error {
            BasicSerialError::OutOfData => MessageSerialError::OutOfData,
            BasicSerialError::BadAscii => MessageSerialError::BadAscii,
            BasicSerialError::MaximumValueExceeded => MessageSerialError::MaximumValueExceeded
        }
    }
}

#[cfg(test)]
mod tests {
    use message::{InventoryVector,KnownNode,Message,Object,GetPubKey,ObjectData,VersionData};
    use net::to_socket_addr;
    use rand::{Rng,SeedableRng,XorShiftRng};
    use serial::message::read::read_message;
    use serial::message::write::write_message;
    use std::io::Cursor;
    use std::time::{Duration,UNIX_EPOCH};

    #[test]
    fn test_addr() {
        let message = Message::Addr {
            addr_list: vec![
                KnownNode {
                    last_seen: UNIX_EPOCH + Duration::from_secs(0x908070605),
                    stream: 2,
                    services: 3,
                    socket_addr: to_socket_addr("12.13.14.15:1617")
                },
                KnownNode {
                    last_seen: UNIX_EPOCH + Duration::from_secs(0x1918171615),
                    stream: 4,
                    services: 5,
                    socket_addr: to_socket_addr("22.23.24.25:2627")
                }
            ]
        };

        let expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            97, 100, 100, 114, // "addr"
            0, 0, 0, 0, 0, 0, 0, 0, // command padding
            0, 0, 0, 77, // payload length
            172, 52, 247, 80, // checksum
            2, // count
            0, 0, 0, 9, 8, 7, 6, 5, // last_seen
            0, 0, 0, 2, // stream
            0, 0, 0, 0, 0, 0, 0, 3, // services
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 12, 13, 14, 15, // ip
            6, 81, // port
            0, 0, 0, 0x19, 0x18, 0x17, 0x16, 0x15, // last_seen
            0, 0, 0, 4, // stream
            0, 0, 0, 0, 0, 0, 0, 5, // services
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 22, 23, 24, 25, // ip
            10, 67 // port
        ];

        run_message_read_write_test(message, expected);
    }

    #[test]
    fn test_getdata() {
        let mut rng: XorShiftRng = SeedableRng::from_seed([0, 0, 0, 1]);
        let hash1: Vec<u8> = rng.gen_iter::<u8>().take(32).collect();
        let hash2: Vec<u8> = rng.gen_iter::<u8>().take(32).collect();

        let message = Message::GetData {
            inventory: vec![
                InventoryVector {
                    hash: hash1.clone()
                },
                InventoryVector {
                    hash: hash2.clone()
                }
            ]
        };

        let mut expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            103, 101, 116, 100, 97, 116, 97, // "getdata"
            0, 0, 0, 0, 0, // command padding
            0, 0, 0, 65, // payload length
            20, 214, 57, 221, // checksum
            2,
        ];
        expected.extend(hash1);
        expected.extend(hash2);

        run_message_read_write_test(message, expected);
    }

    #[test]
    fn test_inv() {
        let mut rng: XorShiftRng = SeedableRng::from_seed([0, 0, 0, 1]);
        let hash1: Vec<u8> = rng.gen_iter::<u8>().take(32).collect();
        let hash2: Vec<u8> = rng.gen_iter::<u8>().take(32).collect();

        let message = Message::Inv {
            inventory: vec![
                InventoryVector {
                    hash: hash1.clone()
                },
                InventoryVector {
                    hash: hash2.clone()
                }
            ]
        };

        let mut expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            105, 110, 118, // "inv"
            0, 0, 0, 0, 0, 0, 0, 0, 0, // command padding
            0, 0, 0, 65, // payload length
            20, 214, 57, 221, // checksum
            2,
        ];
        expected.extend(hash1);
        expected.extend(hash2);

        run_message_read_write_test(message, expected);
    }

    #[test]
    fn test_version() {
        let message = Message::Version(VersionData {
            version: 3,
            services: 1,
            timestamp: UNIX_EPOCH + Duration::from_secs(0x504030201),
            addr_recv: to_socket_addr("127.0.0.1:8444"),
            addr_from: to_socket_addr("11.22.33.44:8555"),
            nonce: 0x12345678,
            user_agent: "Rubbem".to_string(),
            streams: vec![ 1 ]
        });

        let expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            118, 101, 114, 115, 105, 111, 110, // "version"
            0, 0, 0, 0, 0, // command padding
            0, 0, 0, 89, // payload length
            239, 233, 96, 8, // checksum
            0, 0, 0, 3, // version
            0, 0, 0, 0, 0, 0, 0, 1, // services
            0, 0, 0, 5, 4, 3, 2, 1, // timestamp
            0, 0, 0, 0, 0, 0, 0, 1, // recv_services
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1, // recv_addr
            32, 252, // recv_port
            0, 0, 0, 0, 0, 0, 0, 1, // from_services
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 11, 22, 33, 44, // from_addr
            33, 107, // from_port
            0, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, // nonce
            6, 82, 117, 98, 98, 101, 109, // user_agent
            1, 1 // streams
        ];

        run_message_read_write_test(message, expected);
    }

    #[test]
    fn test_verack() {
        let message = Message::Verack;

        let expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            118, 101, 114, 97, 99, 107, // "verack"
            0, 0, 0, 0, 0, 0, // command padding
            0, 0, 0, 0, // payload length
            0xcf, 0x83, 0xe1, 0x35 // checksum
        ];

        run_message_read_write_test(message, expected);
    }

    #[test]
    fn test_object() {
        let mut rng: XorShiftRng = SeedableRng::from_seed([0, 0, 0, 1]);
        let tag: Vec<u8> = rng.gen_iter::<u8>().take(32).collect();

        let message = Message::Object(ObjectData {
            nonce: 0xf29f6e8b9acd981d,
            expiry: UNIX_EPOCH + Duration::from_secs(0x010203040506),
            version: 4, // GetPubKey verion
            stream: 2,
            object: Object::GetPubKey(
                GetPubKey::V4 {
                    tag: tag.clone()
                }
            )
        });

        let mut expected = vec![
            0xe9, 0xbe, 0xb4, 0xd9, // magic
            111, 98, 106, 101, 99, 116, // "object"
            0, 0, 0, 0, 0, 0, // command padding
            0, 0, 0, 54, // payload length
            70, 53, 134, 89, // checksum
            0xf2, 0x9f, 0x6e, 0x8b, 0x9a, 0xcd, 0x98, 0x1d, // nonce
            0, 0, 1, 2, 3, 4, 5, 6, // expiry
            0, 0, 0, 0, // object_type for GetPubKey
            4, // version
            2, // stream
        ];
        expected.extend(tag);

        run_message_read_write_test(message, expected);
    }

    fn run_message_read_write_test(message: Message, expected: Vec<u8>) {
        let mut output = vec![];
        write_message(&mut output, &message);

        assert_eq!(expected, output);

        let mut cursor = Cursor::new(output);
        let roundtrip = read_message(&mut cursor).unwrap();

        assert_eq!(message, roundtrip);
    }
}
