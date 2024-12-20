use std::io;

use bytes::BytesMut;
use monoio::io::{CancelHandle, CancelableAsyncReadRent, CancelableAsyncWriteRent};
pub use tokio_util::codec::Encoder;

use crate::Framed;

/// Decoder may return Decoded to represent a decoded item,
/// or the insufficient length hint, or just the insufficient.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Decoded<T> {
    Some(T),
    // The length needed is unknown.
    // Same as None in tokio_codec::Decoder
    Insufficient,
    // The total length needed.
    InsufficientAtLeast(usize),
}

impl<T> Decoded<T> {
    #[inline]
    pub fn unwrap(self) -> T {
        match self {
            Decoded::Some(inner) => inner,
            Decoded::Insufficient => panic!("unwrap Decoded::Insufficient"),
            Decoded::InsufficientAtLeast(_) => panic!("unwrap Decoded::InsufficientAtLeast"),
        }
    }
}

pub trait Decoder {
    /// The type of decoded frames.
    type Item;

    /// The type of unrecoverable frame decoding errors.
    ///
    /// If an individual message is ill-formed but can be ignored without
    /// interfering with the processing of future messages, it may be more
    /// useful to report the failure as an `Item`.
    ///
    /// `From<io::Error>` is required in the interest of making `Error` suitable
    /// for returning directly from a [`FramedRead`], and to enable the default
    /// implementation of `decode_eof` to yield an `io::Error` when the decoder
    /// fails to consume all available data.
    ///
    /// Note that implementors of this trait can simply indicate `type Error =
    /// io::Error` to use I/O errors as this type.
    ///
    /// [`FramedRead`]: crate::codec::FramedRead
    type Error: From<io::Error>;

    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// This method is called by [`FramedRead`] whenever bytes are ready to be
    /// parsed. The provided buffer of bytes is what's been read so far, and
    /// this instance of `Decode` can determine whether an entire frame is in
    /// the buffer and is ready to be returned.
    ///
    /// If an entire frame is available, then this instance will remove those
    /// bytes from the buffer provided and return them as a decoded
    /// frame. Note that removing bytes from the provided buffer doesn't always
    /// necessarily copy the bytes, so this should be an efficient operation in
    /// most circumstances.
    ///
    /// If the bytes look valid, but a frame isn't fully available yet, then
    /// `Ok(InsufficientUnknown)` is returned. This indicates to the [`Framed`] instance that
    /// it needs to read some more bytes before calling this method again.
    ///
    /// Note that the bytes provided may be empty. If a previous call to
    /// `decode` consumed all the bytes in the buffer then `decode` will be
    /// called again until it returns `Ok(InsufficientUnknown)`, indicating that more bytes need to
    /// be read.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs [`Framed`] that the stream is now
    /// corrupt and should be terminated.
    ///
    /// [`Framed`]: crate::codec::Framed
    /// [`FramedRead`]: crate::codec::FramedRead
    fn decode(&mut self, src: &mut BytesMut) -> Result<Decoded<Self::Item>, Self::Error>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `decode` and returns an error if
    /// `Ok(None)` is returned while there is unconsumed data in `buf`.
    /// Typically this doesn't need to be implemented unless the framing
    /// protocol differs near the end of the stream, or if you need to construct
    /// frames _across_ eof boundaries on sources that can be resumed.
    ///
    /// Note that the `buf` argument may be empty. If a previous call to
    /// `decode_eof` consumed all the bytes in the buffer, `decode_eof` will be
    /// called again until it returns `None`, indicating that there are no more
    /// frames to yield. This behavior enables returning finalization frames
    /// that may not be based on inbound data.
    ///
    /// Once `None` has been returned, `decode_eof` won't be called again until
    /// an attempt to resume the stream has been made, where the underlying stream
    /// actually returned more data.
    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Decoded<Self::Item>, Self::Error> {
        match self.decode(buf)? {
            Decoded::Some(frame) => Ok(Decoded::Some(frame)),
            d => {
                if buf.is_empty() {
                    Ok(d)
                } else {
                    Err(io::Error::new(io::ErrorKind::Other, "bytes remaining on stream").into())
                }
            }
        }
    }

    /// Provides a [`Stream`] and [`Sink`] interface for reading and writing to this
    /// `Io` object, using `Decode` and `Encode` to read and write the raw data.
    ///
    /// Raw I/O objects work with byte sequences, but higher-level code usually
    /// wants to batch these into meaningful chunks, called "frames". This
    /// method layers framing on top of an I/O object, by using the `Codec`
    /// traits to handle encoding and decoding of messages frames. Note that
    /// the incoming and outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both `Stream` and
    /// `Sink`; grouping this into a single object is often useful for layering
    /// things like gzip or TLS, which require both read and write access to the
    /// underlying object.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling `split` on the [`Framed`] returned by this method, which will
    /// break them into separate objects, allowing them to interact more easily.
    ///
    /// [`Stream`]: futures_core::Stream
    /// [`Sink`]: futures_sink::Sink
    /// [`Framed`]: crate::Framed
    fn framed<T: CancelableAsyncReadRent + CancelableAsyncWriteRent + Sized>(
        self,
        io: T,
        cancel_handle: CancelHandle,
    ) -> Framed<T, Self>
    where
        Self: Sized,
    {
        Framed::new(io, self, cancel_handle)
    }
}
