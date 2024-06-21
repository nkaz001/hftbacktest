# Roadmap

Currently, the Rust implementation is being more actively developed especially for new features.

## Python
* [ ] Standardize data format to match the Rust implementation.
* [ ] Rust implementation as the backend.
* [ ] Improve performance reporting tools.

## Rust

### Backtesting
* [X] Level 3 Market-By-Order backtesting (WIP).
* [ ] Data fusion to provide the most frequent and granular data using different streams with different update frequencies and market depth ranges.
* [ ] Support for writing Numpy compressed files and header checking.
* [ ] Adjust feed and order latency for exchanges located in different regions if the original feed and order latency data was collected at a different site.

### Connector
* [ ] Implement Binance Futures Websocket Order APIs; currently, REST APIs are used for submitting orders.
* [ ] Add Binance market depth management mode; currently, only natural refresh is supported.
* [ ] Binance COIN-m Futures/Spot/Options
* [X] Bybit (WIP)
* [ ] OKX
* [ ] Coinbase
* [ ] Kraken
* [ ] CDC
* [ ] Databento for the data feed
* [ ] Trad-fi

### Others
* [ ] Support Level 3 Market-By-Order for Live Bot.
* [ ] Support external connectors through IPC for multiple bots via a unified connection.  
[<img src="https://raw.githubusercontent.com/nkaz001/hftbacktest/master/docs/images/arch.png">](https://github.com/nkaz001/hftbacktest/tree/master/docs/images/arch.png?raw=true)
* [ ] Increase documentation and test coverage.
* [ ] Github workflow.
* [ ] Additional queue position model or exchange model.

### Orchestration
* [ ] Implement interface for live bot orchestration
* [ ] Develop central orchestration app
* [ ] Integrate with Telegram

## Examples
* [ ] Market making example using ARMA, ARIMA, or GARCH on the underlying asset.
* [ ] Example using different skew profiles for inventory management.
* [ ] Example demonstrating latency-aware actions.
* [ ] Example demonstrating the volume clock/event clock using `wait_next_feed`.