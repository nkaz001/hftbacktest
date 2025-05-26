# HftBacktest

![crates.io](https://img.shields.io/crates/v/hftbacktest.svg)

**This project is currently in its initial development stages, meaning that breaking changes may occur without prior
notice. The live bot feature has not undergone comprehensive testing yet; therefore, it must be used at your own risk.**

## High-Frequency Trading Backtesting and Live Bot in Rust

This Rust framework is designed for developing and running high-frequency trading and market-making strategies. It
focuses on accounting for both feed and order latencies, as well as the order queue position for order fill simulation.
The framework aims to provide more accurate market replay-based backtesting, based on full order book and trade tick
feed data. You can also run the live bot using the same algo code.

## Key Features

* Complete tick-by-tick simulation with a customizable time interval or based on the feed and order receipt.
* Full order book reconstruction based on L2 Market-By-Price and L3 Market-By-Order feeds.
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
* Backtesting of multi-asset and multi-exchange models
* Deployment of a live trading bot for quick prototyping and testing using the same algorithm code: currently for Binance Futures and Bybit.

## Getting started

### Installation

**Installation from cargo repository**

    cargo add hftbacktest

**Installation of the up-to-date development version from the sources**

    hftbacktest = { git = "https://github.com/nkaz001/hftbacktest.git" }

### Data Format

The Rust implementation uses a different data format compared to the Python implementation. Please see the Rust version 
part of [Data Preparation](https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html) tutorial.

### Examples

[high-frequency gridtrading](https://github.com/nkaz001/hftbacktest/blob/master/hftbacktest/examples/gridtrading.ipynb): The complete process of backtesting Binance Futures using a high-frequency grid trading strategy implemented in Rust.

Please see [examples](https://github.com/nkaz001/hftbacktest/tree/master/rust/examples).

## Documentation

For general information, please see [document here](https://hftbacktest.readthedocs.io/).  
For the Rust implementation, please see [document here](https://docs.rs/hftbacktest/latest/hftbacktest/).

## Roadmap

Please see the [roadmap](https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md).

## Contributing

Thank you for considering contributing to hftbacktest! Welcome any and all help to improve the project. If you have an
idea for an enhancement or a bug fix, please open an issue or discussion on GitHub to discuss it.

The following items are examples of contributions you can make to this project:

Please see the [roadmap](https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md).