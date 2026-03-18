use crate::{Request, Response};
use bytes::{Buf, BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

const MAX_FRAME_SIZE: u32 = 8 * 1024 * 1024; // 8MB safety limit

pub struct KafkaCodec;

#[derive(Debug)]
pub enum CodecError {
    Io(io::Error),
    Bincode(bincode::Error),
    FrameTooLarge,
}

// Convert CodecError to io::Error to satisfy the Codec traits
impl From<CodecError> for io::Error {
    fn from(err: CodecError) -> Self {
        match err {
            CodecError::Io(e) => e,
            CodecError::Bincode(e) => io::Error::new(io::ErrorKind::InvalidData, e),
            CodecError::FrameTooLarge => {
                io::Error::new(io::ErrorKind::InvalidData, "Frame too large")
            }
        }
    }
}

impl From<io::Error> for CodecError {
    fn from(err: io::Error) -> Self {
        CodecError::Io(err)
    }
}

impl Decoder for KafkaCodec {
    type Item = Request;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let len = u32::from_be_bytes(
            src[..4]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid frame length"))?,
        ) as usize;

        if len > MAX_FRAME_SIZE as usize {
            return Err(CodecError::FrameTooLarge.into());
        }

        if src.len() < 4 + len {
            src.reserve(4 + len - src.len());
            return Ok(None);
        }

        src.advance(4);

        let payload = src.split_to(len);
        let request: Request = bincode::deserialize(&payload).map_err(CodecError::Bincode)?;
        Ok(Some(request))
    }
}

impl Encoder<Response> for KafkaCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Response, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = bincode::serialize(&item).map_err(CodecError::Bincode)?;

        dst.put_u32(payload.len() as u32);

        dst.extend_from_slice(&payload);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use bytes::{BufMut, BytesMut};

    #[test]
    fn test_decode_produce_request() {
        let mut codec = KafkaCodec;
        let mut src = BytesMut::new();

        let request = Request::Produce {
            topic: "test-topic".to_string(),
            message: b"hello kafka-lite".to_vec(),
        };

        let payload = bincode::serialize(&request).expect("Serialization failed");

        src.put_u32(payload.len() as u32);
        src.put_slice(&payload);

        let result = codec.decode(&mut src).expect("Decode failed");
        let decoded = result.expect("Should reuturn Some(Request)");

        match (request, decoded) {
            (
                Request::Produce {
                    topic: t1,
                    message: m1,
                },
                Request::Produce {
                    topic: t2,
                    message: m2,
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(m1, m2);
            }
            _ => panic!("Decoded request did not matche expected vairent or content"),
        }
    }

    #[test]
    fn test_decode_fetch_request() -> io::Result<()> {
        let mut codec = KafkaCodec;
        let mut src = BytesMut::new();

        let request = Request::Fetch {
            topic: "test-topic".to_string(),
            offset: 42,
        };

        let payload = bincode::serialize(&request).expect("Serialize failed");
        src.put_u32(payload.len() as u32);
        src.put_slice(&payload);

        let decode = codec
            .decode(&mut src)?
            .expect("Should return Some(Request)");
        match decode {
            Request::Fetch { topic, offset } => {
                assert_eq!(topic, "test-topic");
                assert_eq!(offset, 42);
            }
            _ => panic!("Expected Fetch request varient"),
        }
        Ok(())
    }

    #[test]
    fn test_encode_produced_response() -> io::Result<()> {
        let mut codec = KafkaCodec;
        let mut dst = BytesMut::new();

        let response = Response::Produced { offset: 123 };
        codec.encode(response, &mut dst).expect("Encode failed");

        assert!(dst.len() > 4);
        let len = u32::from_be_bytes(
            dst[..4]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid len"))?,
        );
        assert_eq!(dst.len(), 4 + len as usize);

        let payload = &dst[4..];
        let decoded: Response = bincode::deserialize(payload).expect("Deserialization failed");
        match decoded {
            Response::Produced { offset } => assert_eq!(offset, 123),
            _ => panic!("Expected Produced response varient"),
        }
        Ok(())
    }

    #[test]
    fn test_decode_partial_frame() -> io::Result<()> {
        let mut codec = KafkaCodec;
        let mut src = BytesMut::new();

        let request = Request::Produce {
            topic: "t".to_string(),
            message: vec![1, 2, 3],
        };
        let payload = bincode::serialize(&request).expect("Serialize failed");
        let total_len = payload.len() as u32;

        // 1. Send only the length - should return None
        src.put_u32(total_len);
        assert!(codec.decode(&mut src)?.is_none());

        // 2. Send partial payload - should return None
        src.put_slice(&payload[..payload.len() - 1]);
        assert!(codec.decode(&mut src)?.is_none());

        // 3. Complete the payload - should return Some(Request)
        src.put_u8(*payload.last().expect("Payload not empty"));
        let result = codec
            .decode(&mut src)?
            .expect("Should complete decoding");

        if let Request::Produce { topic, .. } = result {
            assert_eq!(topic, "t");
        } else {
            panic!("Decoded wrong request type");
        }
        Ok(())
    }

    #[test]
    fn test_decode_frame_too_large() {
        let mut codec = KafkaCodec;
        let mut src = BytesMut::new();

        src.put_u32(MAX_FRAME_SIZE + 1);
        src.put_slice(&[0u8; 10]);

        let result = codec.decode(&mut src);
        assert!(result.is_err());
    }
}
