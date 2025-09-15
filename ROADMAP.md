# Roadmap

## Python
* [X] Rust implementation as the backend.
* [X] Improve performance reporting tools.
  * https://github.com/ranaroussi/quantstats
  * https://www.prestolabs.io/research/optimizing-risk-adjusted-return-in-constructing-portfolios-of-alphas
* [ ] Add more performance metrics and visualization features to the reporting tool.
* [ ] Add live trading support.

## Rust

### Backtesting
* [X] Level 3 Market-By-Order backtesting.
* [X] Data fusion to provide the most frequent and granular data using different streams with different update frequencies and market depth ranges.
* [X] Adjust feed and order latency for exchanges located in different regions if the original feed and order latency data was collected at a different site.
* [ ] Additional queue position model or exchange model.
* [X] A vector-based implementation for fast L2 market depth within the specified ROI (range of interest).
* [X] Add fee model: fee per trading value (current), fee per trading quantity, fee per trade, and different fees based on the direction. (@roykim98)
* [X] Parallel loading: Load the next data set while backtesting is in progress.
* [X] Add a modify order feature.
* [ ] Allow different latencies for placing, modifying, and canceling orders, as well as order responses, fills, and position feeds.
* [ ] Add support for data files in Parquet format.
* [ ] Accelerated backtesting data processor based on the normalized data files.

### Live
* [ ] Support Level 3 Market-By-Order for Live Bot.
* [X] Support external connectors through IPC for multiple bots via a unified connection.
  [<img src="https://raw.githubusercontent.com/nkaz001/hftbacktest/master/docs/images/arch.png">](https://github.com/nkaz001/hftbacktest/tree/master/docs/images/arch.png?raw=true)
  * https://github.com/eclipse-iceoryx/iceoryx2
* [ ] Add a TCP-based communication to support remote connections and the Python version.

### Connector
* [ ] Implement Binance Futures Websocket Order APIs; currently, REST APIs are used for submitting orders.
  * https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-api-general-info
* [ ] Add Binance market depth management mode; currently, only natural refresh is supported.
* [ ] Binance COIN-m Futures/Spot/Options
  * https://developers.binance.com/docs/binance-spot-api-docs/README
  * https://developers.binance.com/docs/derivatives/coin-margined-futures/general-info
  * https://developers.binance.com/docs/derivatives/option/general-info
* [X] Bybit ``MVP``
  * https://bybit-exchange.github.io/docs/v5/intro
* [ ] OKX
  * https://www.okx.com/docs-v5/en/
* [ ] Hyperliquid
* [ ] Coinbase
* [ ] Kraken
* [ ] CDC
* [ ] Databento for the data feed
  * https://databento.com/docs/api-reference-live
* [ ] Trad-fi
* [ ] Split the connector into a market data connector and an order management connector.

### Others
* [ ] Increase documentation and test coverage.
* [ ] Github workflow: readthedocs, build, formatting, coverage, etc.

### Orchestration
* [ ] Implement interface for live bot orchestration
* [ ] Develop central orchestration app
* [ ] Integrate with Telegram

## Examples
* [X] Market making example using ARMA, ARIMA, or GARCH on the underlying asset.
* [ ] Example using different skew profiles for inventory management.
* [ ] Example demonstrating latency-aware actions.
* [ ] Example demonstrating the volume clock/event clock using `wait_next_feed`.
* [ ] Example demonstrating the cross-market market-making.
* [X] Market making with alpha from the perspectives of statistical arbitrage and optimal execution.
* [X] Queue-position-based market making for large-tick assets.
* [X] Update the existing examples to align with version 2.0.0.
* [ ] Example Gymnasium implementation using hftbacktest.