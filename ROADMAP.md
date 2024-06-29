# Roadmap

Currently, the Rust implementation is being more actively developed especially for new features.

## Python
* [X] Rust implementation as the backend. (WIP: [rust_backend](https://github.com/nkaz001/hftbacktest/tree/rust_backend) branch)
  * https://numba.readthedocs.io/en/stable/user/cfunc.html#calling-c-code-from-numba
  * https://numba.readthedocs.io/en/stable/user/cfunc.html#handling-c-structures
  * https://numba.readthedocs.io/en/stable/extending/index.html
  * https://github.com/callahad/python-rust-ffi
  * https://github.com/pyo3/pyo3 
* [ ] Improve performance reporting tools.
  * https://github.com/ranaroussi/quantstats
  * https://www.prestolabs.io/research/optimizing-risk-adjusted-return-in-constructing-portfolios-of-alphas

## Rust

### Backtesting
* [X] Level 3 Market-By-Order backtesting (unstable_l3).
* [X] Data fusion to provide the most frequent and granular data using different streams with different update frequencies and market depth ranges (unstable_fuse)
* [ ] Adjust feed and order latency for exchanges located in different regions if the original feed and order latency data was collected at a different site.

### Connector
* [ ] Implement Binance Futures Websocket Order APIs; currently, REST APIs are used for submitting orders.
  * https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-api-general-info
* [ ] Add Binance market depth management mode; currently, only natural refresh is supported.
* [ ] Binance COIN-m Futures/Spot/Options
  * https://developers.binance.com/docs/binance-spot-api-docs/README
  * https://developers.binance.com/docs/derivatives/coin-margined-futures/general-info
  * https://developers.binance.com/docs/derivatives/option/general-info
* [X] Bybit (MVP)
  * https://bybit-exchange.github.io/docs/v5/intro
* [ ] OKX
  * https://www.okx.com/docs-v5/en/
* [ ] Coinbase
* [ ] Kraken
* [ ] CDC
* [ ] Databento for the data feed
  * https://databento.com/docs/api-reference-live
* [ ] Trad-fi

### Others
* [ ] Support Level 3 Market-By-Order for Live Bot.
* [ ] Support external connectors through IPC for multiple bots via a unified connection.  
[<img src="https://raw.githubusercontent.com/nkaz001/hftbacktest/master/docs/images/arch.png">](https://github.com/nkaz001/hftbacktest/tree/master/docs/images/arch.png?raw=true)
  * https://github.com/eclipse-iceoryx/iceoryx2
* [ ] Increase documentation and test coverage.
* [ ] Github workflow
  * readthedocs, build, formatting, coverage, etc.
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