use crate::buffer::Buffer;
use crate::config::{Config, GetConfig};
use crate::decoder::{Decoder, FieldLocatorContext, Message};
use crate::error::DecodeError;
use crate::raw_decoder::{HeaderInfo, ParserState, RawFrame};
use crate::utils;
use std::marker::PhantomData;

/// Common logic for interfacing with a streaming parser.
///
/// Streaming parsers store incoming bytes in a [`Buffer`] and try to parse
/// them into messages.
///
/// # Errors
///
/// As soon as a single message fails to parse, the whole decoder should be
/// assumed to be in an invalid state. Discard it and create another.
pub trait StreamingDecoder {
    /// The [`Buffer`] implementation used by this decoder.
    type Buffer: Buffer;
    /// The parsing error type.
    type Error;

    /// Returns a mutable reference to the whole internal [`Buffer`].
    fn buffer(&mut self) -> &mut Self::Buffer;

    /// Empties all contents of the internal buffer of `self`.
    fn clear(&mut self) {
        self.buffer().clear();
    }

    /// Provides a lower bound on the number of bytes that are required to reach the end of the
    /// current message.
    fn num_bytes_required(&self) -> usize;

    /// Provides a buffer that must be filled before re-attempting to deserialize
    /// the next message. The slice is *guaranteed* to be non-empty.
    fn fillable(&mut self) -> &mut [u8] {
        let len = self.buffer().len();
        let num_bytes_required = self.num_bytes_required();
        self.buffer().resize(num_bytes_required, 0);
        &mut self.buffer().as_mut_slice()[len..]
    }

    /// Attempts to parse the contents available in the internal [`Buffer`]. The return value gives
    /// you information about the state of the decoder:
    ///
    /// - [`Ok(None)`]: no errors found, but more bytes are required to finish parsing the message.
    /// - [`Ok(Some(()))`]: no errors found, and the message has been fully parsed.
    /// - [`Err`]: parsing failed.
    /// [`StreamingDecoder::Error`] upon failure.
    fn try_parse(&mut self) -> Result<Option<()>, Self::Error>;
}

/// A (de)serializer for the classic FIX tag-value encoding.
///
/// The FIX tag-value encoding is designed to be both human-readable and easy for
/// machines to parse.
///
/// Please reach out to the FIX official documentation[^1][^2] for more information.
///
/// [^1]: [FIX TagValue Encoding: Online reference.](https://www.fixtrading.org/standards/tagvalue-online)
///
/// [^2]: [FIX TagValue Encoding: PDF.](https://www.fixtrading.org/standards/tagvalue/)
#[derive(Debug)]
pub struct DecoderStreaming<B> {
    pub(crate) decoder: Decoder,
    pub(crate) raw_decoder: RawDecoderStreaming<B>,
    pub(crate) is_ready: bool,
}

impl<B> DecoderStreaming<B>
where
    B: Buffer,
{
    /// # Panics
    ///
    /// Panics if [`DecoderStreaming::try_parse()`] didn't return [`Ok(Some(()))`].
    #[inline]
    pub fn message(&self) -> Message<&[u8]> {
        assert!(self.is_ready);

        Message {
            builder: &self.decoder.builder,
            phantom: PhantomData,
            field_locator_context: FieldLocatorContext::TopLevel,
        }
    }
}

impl<B> StreamingDecoder for DecoderStreaming<B>
where
    B: Buffer,
{
    type Buffer = B;
    type Error = DecodeError;

    fn buffer(&mut self) -> &mut Self::Buffer {
        self.raw_decoder.buffer()
    }

    fn clear(&mut self) {
        self.raw_decoder.clear();
        self.is_ready = false;
    }

    fn num_bytes_required(&self) -> usize {
        self.raw_decoder.num_bytes_required()
    }

    fn try_parse(&mut self) -> Result<Option<()>, DecodeError> {
        match self.raw_decoder.try_parse()? {
            Some(()) => {
                self.decoder.from_frame(self.raw_decoder.raw_frame())?;
                self.is_ready = true;
                Ok(Some(()))
            }
            None => Ok(None),
        }
    }
}

impl<B> GetConfig for DecoderStreaming<B> {
    type Config = Config;

    fn config(&self) -> &Self::Config {
        self.decoder.config()
    }

    fn config_mut(&mut self) -> &mut Self::Config {
        self.decoder.config_mut()
    }
}

/// A [`RawDecoder`] that can buffer incoming data and read a stream of messages.
#[derive(Debug)]
pub struct RawDecoderStreaming<B, C = Config> {
    pub(crate) buffer: B,
    pub(crate) config: C,
    pub(crate) state: ParserState,
}

impl<B> StreamingDecoder for RawDecoderStreaming<B>
where
    B: Buffer,
{
    type Buffer = B;
    type Error = DecodeError;

    fn buffer(&mut self) -> &mut B {
        &mut self.buffer
    }

    fn clear(&mut self) {
        self.buffer().clear();
        self.state = ParserState::Empty;
    }

    fn num_bytes_required(&self) -> usize {
        match self.state {
            ParserState::Empty => utils::MIN_FIX_MESSAGE_LEN_IN_BYTES,
            ParserState::Header(_, expected_len) => expected_len,
            ParserState::Failed => 0,
        }
    }

    fn try_parse(&mut self) -> Result<Option<()>, Self::Error> {
        match self.state {
            ParserState::Empty => {
                let header_info =
                    HeaderInfo::parse(self.buffer.as_slice(), self.config().separator);
                if let Some(header_info) = header_info {
                    let expected_len_of_frame = header_info.field_1.end
                        + 1
                        + header_info.nominal_body_len
                        + utils::FIELD_CHECKSUM_LEN_IN_BYTES;

                    self.state = ParserState::Header(header_info, expected_len_of_frame);
                    Ok(None)
                } else {
                    Err(DecodeError::Invalid)
                }
            }
            ParserState::Header(_, _) => Ok(Some(())),
            ParserState::Failed => panic!("Failed state"),
        }
    }
}

impl<B> RawDecoderStreaming<B>
where
    B: Buffer,
{
    /// Tries to deserialize the next [`RawFrame`] from the internal buffer. If
    /// the internal buffer does not contain a complete message, returns an
    /// [`Ok(None)`].
    pub fn raw_frame(&self) -> RawFrame<&[u8]> {
        if let ParserState::Header(header_info, _len) = &self.state {
            let data = &self.buffer.as_slice();

            RawFrame {
                data,
                begin_string: header_info.field_0.clone(),
                payload: header_info.field_1.end + 1
                    ..data.len() - utils::FIELD_CHECKSUM_LEN_IN_BYTES,
            }
        } else {
            panic!("The message is not fully decoded. Check `try_parse` return value.");
        }
    }
}

impl<B, C> GetConfig for RawDecoderStreaming<B, C> {
    type Config = C;

    fn config(&self) -> &C {
        &self.config
    }

    fn config_mut(&mut self) -> &mut C {
        &mut self.config
    }
}
