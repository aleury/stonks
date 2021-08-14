use std::io::{Error, ErrorKind};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Clap;
use futures::future::join_all;
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

struct StockHistory {
    symbol: String,
    closes: Vec<f64>,
}

struct StockStats {
    symbol: String,
    last_price: f64,
    pct_change: f64,
    period_min: f64,
    period_max: f64,
    thirty_day_avg: f64,
}

impl StockStats {
    async fn new(symbol: String, closes: Vec<f64>) -> Self {
        let min_price = MinPrice {};
        let max_price = MaxPrice {};
        let price_diff = PriceDiff {};
        let windowed_sma = WindowedSMA { window_size: 30 };

        let last_price = *closes.last().unwrap_or(&0.0);
        let (_, pct_change) = price_diff.calculate(&closes).await.unwrap();
        let period_min = min_price.calculate(&closes).await.unwrap();
        let period_max = max_price.calculate(&closes).await.unwrap();
        let sma = windowed_sma.calculate(&closes).await.unwrap_or_default();
        let thirty_day_avg = *sma.last().unwrap_or(&0.0);

        StockStats {
            symbol,
            last_price,
            pct_change,
            period_min,
            period_max,
            thirty_day_avg,
        }
    }
}

///
/// A trait to provide a common interface for all signal calculations.
///
#[async_trait]
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
    ///
    async fn calculate(&self, series: &[f64]) -> Option<Self::SignalType>;
}

///
/// Find the minimum in a series of f64.
///
struct MinPrice;

///
/// Find the maximum in a series of f64.
///
struct MaxPrice;

///
/// Calculates the absolute and relative difference between the beginning and ending of a series of f64.
/// The relative difference is relative to the beginning.
///
/// # Returns
///
/// A tuple `(absolute, relative)` difference.
///
struct PriceDiff;

///
/// Calculate a simple moving average of a f64 series.
///
struct WindowedSMA {
    window_size: usize,
}

#[async_trait]
impl StockSignal for MinPrice {
    type SignalType = f64;

    async fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if series.is_empty() {
            return None;
        }

        Some(series.iter().fold(f64::MAX, |acc, x| acc.min(*x)))
    }
}

#[async_trait]
impl StockSignal for MaxPrice {
    type SignalType = f64;

    async fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
        if series.is_empty() {
            return None;
        }

        Some(series.iter().fold(f64::MIN, |acc, x| acc.max(*x)))
    }
}

#[async_trait]
impl StockSignal for PriceDiff {
    type SignalType = (f64, f64);

    async fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
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

#[async_trait]
impl StockSignal for WindowedSMA {
    type SignalType = Vec<f64>;

    async fn calculate(&self, series: &[f64]) -> Option<Self::SignalType> {
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
async fn fetch_closing_data(
    symbol: &str,
    from: &DateTime<Utc>,
    to: &DateTime<Utc>,
) -> std::io::Result<StockHistory> {
    let provider = yahoo::YahooConnector::new();

    let response = provider
        .get_quote_history(symbol, *from, *to)
        .await
        .map_err(|_| Error::from(ErrorKind::InvalidData))?;

    let mut quotes = response
        .quotes()
        .map_err(|_| Error::from(ErrorKind::InvalidData))?;

    let closes: Vec<f64> = if quotes.is_empty() {
        vec![]
    } else {
        quotes.sort_by_cached_key(|q| q.timestamp);
        quotes.iter().map(|q| q.adjclose).collect()
    };

    Ok(StockHistory {
        symbol: symbol.to_owned(),
        closes,
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opts: Opts = Opts::parse();
    let from: DateTime<Utc> = opts.from.parse().expect("Couldn't parse the 'from' date.");
    let to: DateTime<Utc> = Utc::now();
    let symbols = opts.symbols.split(",");

    let stock_histories = join_all(symbols.map(|s| fetch_closing_data(s, &from, &to))).await;

    let stock_stats: Vec<_> = join_all(
        stock_histories
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|s| StockStats::new(s.symbol.to_owned(), s.closes.to_owned())),
    )
    .await;

    println!("period start,symbol,price,change %,min,max,30d avg");
    for stats in stock_stats {
        println!(
            "{},{},${:.2},{:.2}%,${:.2},${:.2},${:.2}",
            from.to_rfc3339(),
            stats.symbol,
            stats.last_price,
            stats.pct_change,
            stats.period_min,
            stats.period_max,
            stats.thirty_day_avg,
        )
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    #[tokio::test]
    async fn test_MinPrice_calculate() {
        let signal = MinPrice {};
        assert_eq!(signal.calculate(&[]).await, None);
        assert_eq!(signal.calculate(&[1.0]).await, Some(1.0));
        assert_eq!(signal.calculate(&[1.0, 0.0]).await, Some(0.0));
        assert_eq!(
            signal
                .calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0])
                .await,
            Some(1.0)
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]).await,
            Some(0.0)
        );
    }

    #[tokio::test]
    async fn test_MaxPrice_calculate() {
        let signal = MaxPrice {};
        assert_eq!(signal.calculate(&[]).await, None);
        assert_eq!(signal.calculate(&[1.0]).await, Some(1.0));
        assert_eq!(signal.calculate(&[1.0, 0.0]).await, Some(1.0));
        assert_eq!(
            signal
                .calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0])
                .await,
            Some(10.0)
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]).await,
            Some(6.0)
        );
    }

    #[tokio::test]
    async fn test_PriceDiff_calculate() {
        let signal = PriceDiff {};
        assert_eq!(signal.calculate(&[]).await, None);
        assert_eq!(signal.calculate(&[1.0]).await, Some((0.0, 0.0)));
        assert_eq!(signal.calculate(&[1.0, 0.0]).await, Some((-1.0, -1.0)));
        assert_eq!(
            signal
                .calculate(&[2.0, 3.0, 5.0, 6.0, 1.0, 2.0, 10.0])
                .await,
            Some((8.0, 4.0))
        );
        assert_eq!(
            signal.calculate(&[0.0, 3.0, 5.0, 6.0, 1.0, 2.0, 1.0]).await,
            Some((1.0, 1.0))
        );
    }

    #[tokio::test]
    async fn test_WindowedSMA_calculate() {
        let series = vec![2.0, 4.5, 5.3, 6.5, 4.7];

        let signal = WindowedSMA { window_size: 3 };
        assert_eq!(
            signal.calculate(&series).await,
            Some(vec![3.9333333333333336, 5.433333333333334, 5.5])
        );

        let signal = WindowedSMA { window_size: 5 };
        assert_eq!(signal.calculate(&series).await, Some(vec![4.6]));

        let signal = WindowedSMA { window_size: 10 };
        assert_eq!(signal.calculate(&series).await, Some(vec![]));
    }
}
