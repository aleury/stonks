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

fn main() -> std::io::Result<()> {
    let opts: Opts = Opts::parse();
    let from: DateTime<Utc> = opts.from.parse().expect("Couldn't parse the 'from' date.");
    let to: DateTime<Utc> = Utc::now();

    println!("period start,symbol,price,change %,min,max,30d avg");
    for symbol in opts.symbols.split(",") {
        let closes = fetch_closing_data(symbol, &from, &to)?;
        if !closes.is_empty() {
            let last_price = *closes.last().unwrap_or(&0.0);
            let (_, pct_change) = price_diff(&closes).unwrap();
            let period_min = min(&closes).unwrap();
            let period_max = max(&closes).unwrap();
            let sma = n_window_sma(30, &closes).unwrap_or_default();

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

///
/// Find the minimum ina series of f64.
///
fn min(series: &[f64]) -> Option<f64> {
    if series.is_empty() {
        return None;
    }

    Some(series.iter().fold(f64::MAX, |acc, x| acc.min(*x)))
}

///
/// Find the maximum in a series of f64.
///
fn max(series: &[f64]) -> Option<f64> {
    if series.is_empty() {
        return None;
    }

    Some(series.iter().fold(f64::MIN, |acc, x| acc.max(*x)))
}

///
/// Calculates the absolute and relative difference between the beginning and ending of a series of f64.
/// The relative difference is relative to the beginning.
///
/// # Returns
///
/// A tuple `(absolute, relative)` difference.
///
fn price_diff(series: &[f64]) -> Option<(f64, f64)> {
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

fn n_window_sma(n: usize, series: &[f64]) -> Option<Vec<f64>> {
    if series.is_empty() {
        return None;
    }

    let results = series
        .windows(n)
        .map(|w| w.iter().sum::<f64>() / w.len() as f64)
        .collect();

    Some(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min() {
        assert_eq!(min(&[]), None);
        assert_eq!(min(&[1.0]), Some(1.0));
        assert_eq!(min(&[1.0, 0.0]), Some(0.0));
        assert_eq!(min(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]), Some(1.0));
        assert_eq!(min(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]), Some(0.0));
    }

    #[test]
    fn test_max() {
        assert_eq!(max(&[]), None);
        assert_eq!(max(&[1.0]), Some(1.0));
        assert_eq!(max(&[1.0, 0.0]), Some(1.0));
        assert_eq!(max(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]), Some(10.0));
        assert_eq!(max(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]), Some(6.0));
    }

    #[test]
    fn test_price_diff() {
        assert_eq!(price_diff(&[]), None);
        assert_eq!(price_diff(&[1.0]), Some((0.0, 0.0)));
        assert_eq!(price_diff(&[1.0, 0.0]), Some((-1.0, -1.0)));
        assert_eq!(
            price_diff(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0]),
            Some((8.0, 4.0))
        );
        assert_eq!(
            price_diff(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]),
            Some((1.0, 1.0))
        );
    }

    #[test]
    fn test_n_window_sma() {
        let series = vec![2.0, 4.5, 5.3, 6.5, 4.7];

        assert_eq!(
            n_window_sma(3, &series),
            Some(vec![3.9333333333333336, 5.433333333333334, 5.5])
        );

        assert_eq!(n_window_sma(5, &series), Some(vec![4.6]));

        assert_eq!(n_window_sma(10, &series), Some(vec![]));
    }
}
