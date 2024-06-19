# Roadmap

Currently, the Rust implementation is being more actively developed especially for new features.

## Python
* [ ] Standardize data format to match the Rust implementation.
* [ ] Rust implementation as the backend.
* [ ] Improve performance reporting tools.

## Rust

### Backtesting
* [ ] Level 3 Market-By-Order backtesting (WIP).
* [ ] Data fusion to provide the most frequent and granular data using different streams with different update frequencies and market depth ranges.
* [ ] Support for writing Numpy compressed files and header checking.

### Connector
* [ ] Implement Binance Futures Websocket Order APIs; currently, REST APIs are used for submitting orders.
* [ ] Binance COIN-m Futures/Spot/Options
* [ ] Bybit (WIP)
* [ ] OKX
* [ ] Coinbase
* [ ] Kraken
* [ ] CDC
* [ ] Databento for the data feed
* [ ] Trad-fi

### Others
* [ ] Support Level 3 Market-By-Order for Live Bot.
* [ ] Support external connectors through IPC for multiple bots via a unified connection.  
[<img src="https://github.com/nkaz001/hftbacktest/tree/master/docs/images/arch.png?raw=true">](https://github.com/nkaz001/hftbacktest/tree/master/docs/images/arch.png?raw=true)
* [ ] Telegram bot integration.
* [ ] Increase documentation and test coverage.
* [ ] Github workflow.
* [ ] Additional queue position model or exchange model.

## Examples
* [ ] Market making example using ARMA, ARIMA, or GARCH on the underlying asset.
* [ ] Example using different skew profiles for inventory management.
* [ ] Example demonstrating latency-aware actions.
* [ ] Example demonstrating the volume clock/event clock using `wait_next_feed`.