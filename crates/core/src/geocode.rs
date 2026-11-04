use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::exif::Gps;

/// OpenStreetMap's free search. Its policy asks for an identifying User-Agent and at most one
/// request per second: https://operations.osmfoundation.org/policies/nominatim/
const NOMINATIM: &str = "https://nominatim.openstreetmap.org/search";
const USER_AGENT: &str = concat!("Lapsewright/", env!("CARGO_PKG_VERSION"), " (https://github.com/diopterme/lapsewright)");
const MIN_INTERVAL: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESULTS: &str = "5";

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub gps: Gps,
}

/// Keep one per app: the one-second spacing is tracked here.
pub struct Geocoder {
    base_url: String,
    timeout: Duration,
    last_call: Option<Instant>,
}

impl Default for Geocoder {
    fn default() -> Self {
        Geocoder::at(NOMINATIM, TIMEOUT)
    }
}

impl Geocoder {
    fn at(base_url: &str, timeout: Duration) -> Self {
        Geocoder { base_url: base_url.to_string(), timeout, last_call: None }
    }

    /// Up to five places for a name; waits if the previous search was under a second ago.
    pub fn search(&mut self, query: &str) -> Result<Vec<Place>, GeocodeError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(wait) = self.last_call.and_then(|last| MIN_INTERVAL.checked_sub(last.elapsed())) {
            thread::sleep(wait);
        }
        // Counted from the attempt, so a failed search still spaces the next one.
        self.last_call = Some(Instant::now());
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(self.timeout)).build().into();
        let found: Vec<Found> = agent
            .get(&self.base_url)
            .header("User-Agent", USER_AGENT)
            .query("format", "jsonv2")
            .query("q", query)
            .query("limit", MAX_RESULTS)
            .call()
            .and_then(|mut response| response.body_mut().read_json())
            .map_err(GeocodeError::from)?;
        found.into_iter().map(Found::into_place).collect()
    }
}

/// Nominatim sends coordinates as strings.
#[derive(Deserialize)]
struct Found {
    lat: String,
    lon: String,
    display_name: String,
}

impl Found {
    fn into_place(self) -> Result<Place, GeocodeError> {
        let degrees = |text: &str, limit: f64| text.parse::<f64>().ok().filter(|v| v.abs() <= limit);
        match (degrees(&self.lat, 90.0), degrees(&self.lon, 180.0)) {
            (Some(lat), Some(lon)) => Ok(Place { name: self.display_name, gps: Gps { lat, lon } }),
            _ => Err(GeocodeError::BadResponse(format!("coordinates `{}`, `{}`", self.lat, self.lon))),
        }
    }
}

/// Why a search failed, so the screen can say "offline" apart from "server refused".
#[derive(Debug, thiserror::Error)]
pub enum GeocodeError {
    #[error("network: {0}")]
    Network(String),
    #[error("no answer within {} seconds", TIMEOUT.as_secs())]
    Timeout,
    /// 429 = too many searches, 403 = blocked.
    #[error("search server answered HTTP {0}")]
    Status(u16),
    #[error("unexpected answer: {0}")]
    BadResponse(String),
}

impl From<ureq::Error> for GeocodeError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(code) => GeocodeError::Status(code),
            ureq::Error::Timeout(_) => GeocodeError::Timeout,
            ureq::Error::Json(e) => GeocodeError::BadResponse(e.to_string()),
            other => GeocodeError::Network(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    const QUICK: Duration = Duration::from_millis(300);

    fn ok(body: &str) -> Option<String> {
        Some(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()))
    }

    fn status(code: u16) -> Option<String> {
        Some(format!("HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"))
    }

    /// Answers each connection with the next response (`None` = never answers) and reports
    /// when each request arrived and what it said.
    fn serve(responses: Vec<Option<String>>) -> (String, mpsc::Receiver<(Instant, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/search", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                tx.send((Instant::now(), String::from_utf8_lossy(&buf[..n]).into_owned())).unwrap();
                match response {
                    Some(text) => stream.write_all(text.as_bytes()).unwrap(),
                    None => thread::sleep(QUICK * 3),
                }
            }
        });
        (url, rx)
    }

    const ANDONG: &str = r#"[{"lat":"36.5970","lon":"128.8720","display_name":"안동호, 안동시, 경상북도, 대한민국"}]"#;

    #[test]
    fn asks_with_the_app_user_agent_and_reads_places() {
        let (url, requests) = serve(vec![ok(ANDONG)]);
        let places = Geocoder::at(&url, QUICK).search("안동호").unwrap();
        assert_eq!(places, [Place { name: "안동호, 안동시, 경상북도, 대한민국".into(), gps: Gps { lat: 36.597, lon: 128.872 } }]);
        let (_, request) = requests.recv().unwrap();
        let lower = request.to_lowercase();
        assert!(lower.starts_with("get /search?"), "{request}");
        for part in ["format=jsonv2", "limit=5", "q=%ec%95%88%eb%8f%99%ed%98%b8", "user-agent: lapsewright/"] {
            assert!(lower.contains(part), "{part} missing in {request}");
        }
    }

    #[test]
    fn second_search_waits_a_second_after_the_first() {
        let (url, requests) = serve(vec![status(503), ok("[]")]);
        let mut geocoder = Geocoder::at(&url, QUICK);
        assert!(geocoder.search("a").is_err());
        assert_eq!(geocoder.search("b").unwrap(), []);
        let (first, _) = requests.recv().unwrap();
        let (second, _) = requests.recv().unwrap();
        assert!(second - first >= Duration::from_millis(990), "{:?}", second - first);
    }

    #[test]
    fn failures_say_why() {
        let (url, _requests) = serve(vec![status(429), ok("{ nope"), ok(r#"[{"lat":"91","lon":"0","display_name":"x"}]"#), None]);
        let mut geocoder = Geocoder::at(&url, QUICK);
        assert!(matches!(geocoder.search("a"), Err(GeocodeError::Status(429))));
        assert!(matches!(geocoder.search("a"), Err(GeocodeError::BadResponse(_))));
        assert!(matches!(geocoder.search("a"), Err(GeocodeError::BadResponse(_))));
        assert!(matches!(geocoder.search("a"), Err(GeocodeError::Timeout)));
    }

    #[test]
    fn closed_server_is_a_network_error_and_blank_query_asks_nobody() {
        let url = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/search", listener.local_addr().unwrap())
        };
        let mut geocoder = Geocoder::at(&url, QUICK);
        assert_eq!(geocoder.search("  ").unwrap(), []);
        assert!(matches!(geocoder.search("a"), Err(GeocodeError::Network(_))));
    }
}
