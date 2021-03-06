use av_data::params::{CodecParams, MediaKind, VideoInfo};
use av_data::rational::Rational64;
use av_format::buffer::Buffered;
use av_format::common::GlobalInfo;
use av_format::demuxer::{Demuxer, Event};
use av_format::demuxer::{Descr, Descriptor};
use av_format::error::*;
use av_format::stream::Stream;
use nom::bytes::complete::tag;
use nom::bytes::complete::take_till;
use nom::combinator::map_res;
use nom::{IResult, Offset};
use std::collections::VecDeque;
use std::io::SeekFrom;

#[derive(Default)]
struct Y4MDemuxer {
    header: Option<Y4MHeader>,
    queue: VecDeque<Event>,
}

#[derive(Default, Clone, Debug)]
pub struct Y4MHeader {
    width: usize,
    height: usize,
}

impl Y4MDemuxer {
    pub fn new() -> Y4MDemuxer {
        Default::default()
    }
}

impl Demuxer for Y4MDemuxer {
    fn read_headers(&mut self, buf: &Box<dyn Buffered>, info: &mut GlobalInfo) -> Result<SeekFrom> {
        match header(buf.data()) {
            Ok((input, header)) => {
                debug!("found header: {:?}", header);
                let st = Stream {
                    id: 0,
                    index: 0,
                    params: CodecParams {
                        extradata: None,
                        codec_id: None,
                        bit_rate: 1,
                        delay: 0,
                        convergence_window: 0,
                        kind: Some(MediaKind::Video(VideoInfo {
                            width: header.width,
                            height: header.height,
                            format: None,
                        })),
                    },
                    start: None,
                    duration: None,
                    timebase: Rational64::new(1, 1000 * 1000 * 1000),
                    user_private: None,
                };
                self.header = Some(header);
                info.add_stream(st);
                Ok(SeekFrom::Current(buf.data().offset(input) as i64))
            }
            Err(e) => {
                error!("error reading headers: {:?}", e);
                Err(Error::InvalidData)
            }
        }
    }

    fn read_event(&mut self, buf: &Box<dyn Buffered>) -> Result<(SeekFrom, Event)> {
        if let Some(event) = self.queue.pop_front() {
            Ok((SeekFrom::Current(0), event))
        } else {
            // check for EOF
            if buf.data().is_empty() {
                return Ok((SeekFrom::Current(0), Event::Eof));
            }

            // TODO implement
            Err(Error::InvalidData)
        }
    }
}

fn from_utf8(input: &[u8]) -> std::result::Result<&str, std::str::Utf8Error> {
    std::str::from_utf8(input)
}

fn header_token(input: &[u8]) -> IResult<&[u8], &str> {
    map_res(take_till(|c| c == b' '), from_utf8)(input).map(|(i, token_str)| {
        // Remove space from input
        (&i[1..], token_str)
    })
}

fn header(input: &[u8]) -> IResult<&[u8], Y4MHeader> {
    let mut header = Y4MHeader::default();
    let (mut i, _) = tag("YUV4MPEG2 ")(input)?;

    loop {
        let (ii, token) = header_token(i)?;
        let (id, val) = token.split_at(1);
        i = ii;
        match id {
            "W" => header.width = val.parse::<usize>().unwrap_or(0),
            "H" => header.height = val.parse::<usize>().unwrap_or(0),
            _ => break,
        }
    }

    Ok((i, header))
}

struct Des {
    d: Descr,
}

impl Descriptor for Des {
    fn create(&self) -> Box<dyn Demuxer> {
        Box::new(Y4MDemuxer::new())
    }
    fn describe(&self) -> &Descr {
        &self.d
    }
    fn probe(&self, data: &[u8]) -> u8 {
        match header(&data[..=10]) {
            Ok(_) => 10,
            _ => 0,
        }
    }
}

/// used by av context
pub const Y4M_DESC: &dyn Descriptor = &Des {
    d: Descr {
        name: "y4m-rs",
        demuxer: "y4m",
        description: "Nom-based Y4M demuxer",
        extensions: &["y4m"],
        mime: &[],
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use av_format::buffer::AccReader;
    use av_format::demuxer::Context;
    use std::io::Cursor;

    const Y4M: &[u8] = include_bytes!("../assets/test.y4m");

    #[test]
    fn parse_headers() {
        let _ = pretty_env_logger::try_init();

        let descriptor = Y4M_DESC.create();
        let cursor = Cursor::new(Y4M);
        let acc = AccReader::new(cursor);
        let input = Box::new(acc);

        let mut demuxer = Context::new(descriptor, input);

        match demuxer.read_headers() {
            Ok(_) => debug!("Headers read correctly"),
            Err(e) => {
                panic!("error: {:?}", e);
            }
        }

        trace!("global info: {:#?}", demuxer.info);
    }

    #[test]
    fn demux() {
        let _ = pretty_env_logger::try_init();
        let descriptor = Y4M_DESC.create();
        let cursor = Cursor::new(Y4M);
        let acc = AccReader::new(cursor);
        let input = Box::new(acc);
        let mut demuxer = Context::new(descriptor, input);
        demuxer.read_headers().unwrap();

        trace!("global info: {:#?}", demuxer.info);

        loop {
            match demuxer.read_event() {
                Ok(event) => match event {
                    Event::MoreDataNeeded(sz) => panic!("we needed more data: {} bytes", sz),
                    Event::NewStream(s) => panic!("new stream :{:?}", s),
                    Event::NewPacket(packet) => {
                        debug!("received packet with pos: {:?}", packet.pos);
                    }
                    Event::Continue => continue,
                    Event::Eof => {
                        debug!("EOF!");
                        break;
                    }
                    _ => unimplemented!(),
                },
                Err(e) => {
                    panic!("error: {:?}", e);
                }
            }
        }
    }
}
