use futures::{stream, StreamExt};
use clap::{Parser, error};
use log::{debug, error, info, log_enabled, warn};
use std::io::Write;
use std::collections::LinkedList;
use std::{error::Error, fmt};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, FixedOffset};
use chrono::format::ParseError;
use quick_xml::events::Event;
use quick_xml::reader::Reader;


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// URL to podcast RSS feed.
    #[clap(short, long)]
    rss_url: String,

    /// Number of tokio tasks to use while performing downloads.
    #[clap(short, long, default_value = "4")]
    task_count: usize,

    /// Use syslog.
   #[clap(long, action)]
   syslog: bool
}

#[derive(Debug, Clone)]
struct Episode {
    url: String,
    title: String,
    datetime: NaiveDateTime,

    raw: String,
}

#[derive(Debug)]
struct RssFormatError {
    text: String
}

impl Error for RssFormatError {}

impl fmt::Display for RssFormatError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oh no, something bad went down")
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    init_logging(args.syslog);

    let rss_xml = reqwest::get(args.rss_url)
        .await?
        .text()
        .await?;

    let episodes = parse_rss(&rss_xml).unwrap();

    let client = reqwest::Client::new();

    let bodies = stream::iter(episodes)
        .map(|episode| {
            let client = client.clone();
            let episode_clone = episode.clone();
            tokio::spawn(async move {
                let resp = client.get(episode.url).send().await?;
                let data = match resp.bytes().await {
                    Ok(data) => data,
                    Err(e) => return Err(e)
                };
                // Ok((episode.clone(), data))
                Ok((episode_clone, data))
            })
        })
        .buffer_unordered(args.task_count);

    bodies
        .for_each(|b| async {
            match b {
                Ok(Ok(b)) => {info!("Got {} bytes", b.1.len())},
                Ok(Err(e)) => error!("Got a reqwest::Error: {}", e),
                Err(e) => error!("Got a tokio::JoinError: {}", e),
            }
        })
        .await;

    Ok(())
}

fn init_logging(use_syslog: bool) {
    let mut log_builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"));

    if use_syslog {
        log_builder.format(|buffer, record| {
            writeln!(buffer, "<{}>{}", record.level() as u8 + 2 , record.args())
        });
    }
    log_builder.init();
}

fn parse_rss(rss_xml: &str) -> Result<LinkedList<Episode>, Box<dyn Error>> {
    let mut reader = Reader::from_str(rss_xml);
    reader.trim_text(true);

    let mut list_of_events = LinkedList::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"item" => {
                let txt = reader
                    .read_text(e.name())
                    .expect("Cannot decode text value");
                if let Ok(episode) = parse_item(txt.as_ref()) {
                    list_of_events.push_back(episode);
                }
                else {
                    error!("Could not parse episode");
                }
                
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                error!("Error at position {}: {:?}", reader.buffer_position(), e);
                return Err(Box::new(e))
            },
            _ => ()
        }
    }

    Ok(list_of_events)
}

fn parse_item(item_xml: &str) -> Result<Episode, Box<dyn Error>> {
    let mut reader = Reader::from_str(item_xml);
    let mut title: Option<String> = None;
    let mut datetime: Option<NaiveDateTime> = None;
    let mut url: Option<String> = None;

    reader.expand_empty_elements(true);
    
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                // info!("At element, {}", std::str::from_utf8(element.name().as_ref()).unwrap());
                if element.name().as_ref() == b"title" {
                    let txt = match reader.read_text(element.name()) {
                        Ok(txt) => txt,
                        Err(e) => return Err(Box::new(e))
                    };

                    title = Some(txt.to_string());
                }
                else if element.name().as_ref() == b"pubDate" {
                    let txt = match reader.read_text(element.name()) {
                        Ok(txt) => txt,
                        Err(e) => return Err(Box::new(e))
                    };

                    datetime = match parse_date_time(txt.as_ref()) {
                        Ok(datetime) => Some(datetime.naive_local()),
                        Err(e) => return Err(Box::new(e))
                    }
                }
                else if element.name().as_ref() == b"enclosure" {
                    for attr_result in element.attributes() {
                        let attr = attr_result?;
                        match attr.key.as_ref() {
                            b"url" => url = Some(attr.decode_and_unescape_value(&reader)?.to_string()),
                            _ => (),
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                error!("Error at position {}: {:?}", reader.buffer_position(), e);
                return Err(Box::new(e))
            },
            _ => ()
        }
    }

    if url.is_none() || title.is_none() || datetime.is_none() {
        error!("Missing episode properties");
        return Err(Box::new(RssFormatError{ text: item_xml.to_string() }))
    }

    Ok(Episode{url: url.unwrap().clone(), title: title.unwrap().clone(), datetime: datetime.unwrap().clone(), raw: item_xml.to_string()})
}

fn parse_date_time(datetime_str: &str) -> Result<DateTime<FixedOffset>, ParseError> {
    DateTime::parse_from_rfc2822(&datetime_str)
}
