use std::io::{Error, ErrorKind};

use chrono::{DateTime, Utc};
use clap::Clap;
use yahoo_finance_api as yahoo;

#[derive(clap::Clap)]
#[clap(
    version = "0.1.0",
    author = "Adam Eury",
    about = "Track stonk prices with ease!"
)]
struct Opts {
    #[clap(short, long, default_value = "AAPL,MSFT,UBER,GOOG")]
    symbols: String,
    #[clap(short, long)]
    from: String,
}

///
/// A trait to provide a common interface for all signal calculations.
///
trait StockSignal {
    ///
    /// The signals data type.
    ///
    type SignalType;

    ///
    /// Calculate the signal on the provided series.
    ///
    /// # Returns
    ///
    /// The signal (using the provided typep) or `None` on error/invalid data.
    fn calculate(&self, series: &[f64]) -> Option<Self::SignalType>;
}

///
/// Find the minimum in a series of f64.
///
struct MinPrice;

impl StockSignal for MinPrice {
    type SignalType = f64;

    fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if series.is_empty() {
            return None;
        }

        Some(series.iter().fold(f64::MAX, |acc, x| acc.min(*x)))
    }
}

///
/// Find the maximum in a series of f64.
///
struct MaxPrice;

impl StockSignal for MaxPrice {
    type SignalType = f64;

    fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if series.is_empty() {
            return None;
        }

        Some(series.iter().fold(f64::MIN, |acc, x| acc.max(*x)))
    }
}

///
/// Calculates the absolute and relative difference between the beginning and ending of a series of f64.
/// The relative difference is relative to the beginning.
///
/// # Returns
///
/// A tuple `(absolute, relative)` difference.
///
struct PriceDiff;

impl StockSignal for PriceDiff {
    type SignalType = (f64, f64);

    fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if series.is_empty() {
            return None;
        }

        let first = series.first().map(|x| *x).unwrap();
        let last = series.last().map(|x| *x).unwrap();

        let abs_diff = last - first;

        let pct_diff = if first == 0.0 {
            abs_diff
        } else {
            abs_diff / first
        };

        Some((abs_diff, pct_diff))
    }
}

struct WindowedSMA {
    window_size: usize,
}

impl StockSignal for WindowedSMA {
    type SignalType = Vec<f64>;

    fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if !series.is_empty() && self.window_size > 1 {
            Some(
                series
                    .windows(self.window_size)
                    .map(|w| w.iter().sum::<f64>() / w.len() as f64)
                    .collect(),
            )
        } else {
            None
        }
    }
}

///
/// Fetch the closing prices of a stonk over a period of time.
///
fn fetch_closing_data(
    symbol: &str,
    from: &DateTime<Utc>,
    to: &DateTime<Utc>,
) -> std::io::Result<Vec<f64>> {
    let provider = yahoo::YahooConnector::new();

    let response = provider
        .get_quote_history(symbol, *from, *to)
        .map_err(|_| Error::from(ErrorKind::InvalidData))?;

    let mut quotes = response
        .quotes()
        .map_err(|_| Error::from(ErrorKind::InvalidData))?;

    if quotes.is_empty() {
        return Ok(vec![]);
    }

    quotes.sort_by_cached_key(|q| q.timestamp);
    Ok(quotes.iter().map(|q| q.adjclose).collect())
}

fn main() -> std::io::Result<()> {
    let opts: Opts = Opts::parse();
    let from: DateTime<Utc> = opts.from.parse().expect("Couldn't parse the 'from' date.");
    let to: DateTime<Utc> = Utc::now();

    println!("period start,symbol,price,change %,min,max,30d avg");
    for symbol in opts.symbols.split(",") {
        let closes = fetch_closing_data(symbol, &from, &to)?;
        if !closes.is_empty() {
            let min_price = MinPrice {};
            let max_price = MaxPrice {};
            let price_diff = PriceDiff {};
            let windowed_sma = WindowedSMA { window_size: 30 };

            let last_price = *closes.last().unwrap_or(&0.0);
            let (_, pct_change) = price_diff.calculate(&closes).unwrap();
            let period_min = min_price.calculate(&closes).unwrap();
            let period_max = max_price.calculate(&closes).unwrap();
            let sma = windowed_sma.calculate(&closes).unwrap_or_default();

            println!(
                "{},{},${:.2},{:.2}%,${:.2},${:.2},${:.2}",
                from.to_rfc3339(),
                symbol,
                last_price,
                pct_change,
                period_min,
                period_max,
                sma.last().unwrap_or(&0.0),
            )
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn test_MinPrice_calculate() {
        let signal = MinPrice {};
        assert_eq!(signal.calculate(&[]), None);
        assert_eq!(signal.calculate(&[1.0]), Some(1.0));
        assert_eq!(signal.calculate(&[1.0, 0.0]), Some(0.0));
        assert_eq!(
            signal.calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]),
            Some(1.0)
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]),
            Some(0.0)
        );
    }

    #[test]
    fn test_MaxPrice_calculate() {
        let signal = MaxPrice {};
        assert_eq!(signal.calculate(&[]), None);
        assert_eq!(signal.calculate(&[1.0]), Some(1.0));
        assert_eq!(signal.calculate(&[1.0, 0.0]), Some(1.0));
        assert_eq!(
            signal.calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]),
            Some(10.0)
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]),
            Some(6.0)
        );
    }

    #[test]
    fn test_PriceDiff_calculate() {
        let signal = PriceDiff {};
        assert_eq!(signal.calculate(&[]), None);
        assert_eq!(signal.calculate(&[1.0]), Some((0.0, 0.0)));
        assert_eq!(signal.calculate(&[1.0, 0.0]), Some((-1.0, -1.0)));
        assert_eq!(
            signal.calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]),
            Some((8.0, 4.0))
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]),
            Some((1.0, 1.0))
        );
    }

    #[test]
    fn test_WindowedSMA_calculate() {
        let series = vec![2.0, 4.5, 5.3, 6.5, 4.7];

        let signal = WindowedSMA { window_size: 3 };
        assert_eq!(
            signal.calculate(&series),
            Some(vec![3.9333333333333336, 5.433333333333334, 5.5])
        );

        let signal = WindowedSMA { window_size: 5 };
        assert_eq!(signal.calculate(&series), Some(vec![4.6]));

        let signal = WindowedSMA { window_size: 10 };
        assert_eq!(signal.calculate(&series), Some(vec![]));
    }
}
