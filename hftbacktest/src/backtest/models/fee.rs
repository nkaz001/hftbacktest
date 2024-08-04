use crate::{prelude::Side, types::Order};

/// Common transaction fees
/// Fee calculation is determined by the fee model.
#[derive(Clone)]
pub struct CommonFees {
    /// Fee for adding liquidity (maker order).
    maker_fee: f64,
    /// Fee for removing liquidity (taker order).
    taker_fee: f64,
}

impl CommonFees {
    /// Constructs `CommonFees`.
    pub fn new(maker_fee: f64, taker_fee: f64) -> Self {
        Self {
            maker_fee,
            taker_fee,
        }
    }
}

/// Directional fees, such as stamp duty, are typically charged based on the transaction value in
/// addition to the common transaction fees.
#[derive(Clone)]
pub struct DirectionalFees {
    /// The common transaction fees
    common_fees: CommonFees,
    /// Buyer fee based on the transaction value
    buyer_fee: f64,
    /// Seller fee based on the transaction value
    seller_fee: f64,
}

impl DirectionalFees {
    /// Constructs `DirectionalFees`.
    pub fn new(common_fees: CommonFees, buyer_fee: f64, seller_fee: f64) -> Self {
        Self {
            common_fees,
            buyer_fee,
            seller_fee,
        }
    }
}

/// Provides the fee.
pub trait FeeModel {
    /// Calculates the fee amount.
    fn amount(&self, order: &Order, amount: f64) -> f64;
}

/// Fee based on the transaction value,
/// with the rate depending on whether the order is a maker or taker.
#[derive(Clone)]
pub struct TradingValueFeeModel<Fees> {
    fees: Fees,
}

impl<Fees> TradingValueFeeModel<Fees> {
    /// Constructs `TradingValueFeeModel`.
    pub fn new(fees: Fees) -> Self {
        Self { fees }
    }
}

impl FeeModel for TradingValueFeeModel<CommonFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        if order.maker {
            self.fees.maker_fee * amount
        } else {
            self.fees.taker_fee * amount
        }
    }
}

impl FeeModel for TradingValueFeeModel<DirectionalFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        match (order.maker, order.side) {
            (true, Side::Buy) => (self.fees.common_fees.maker_fee + self.fees.buyer_fee) * amount,
            (false, Side::Buy) => (self.fees.common_fees.taker_fee + self.fees.buyer_fee) * amount,
            (true, Side::Sell) => (self.fees.common_fees.maker_fee + self.fees.seller_fee) * amount,
            (false, Side::Sell) => {
                (self.fees.common_fees.taker_fee + self.fees.seller_fee) * amount
            }
            _ => unreachable!(),
        }
    }
}

/// Fee based on the transaction quantity,
/// with the rate depending on whether the order is a maker or taker.
#[derive(Clone)]
pub struct TradingQtyFeeModel<Fees> {
    fees: Fees,
}

impl<Fees> TradingQtyFeeModel<Fees> {
    /// Constructs `TradingQtyFeeModel`.
    pub fn new(fees: Fees) -> Self {
        Self { fees }
    }
}
impl FeeModel for TradingQtyFeeModel<CommonFees> {
    fn amount(&self, order: &Order, _amount: f64) -> f64 {
        if order.maker {
            self.fees.maker_fee * order.exec_qty
        } else {
            self.fees.taker_fee * order.exec_qty
        }
    }
}

impl FeeModel for TradingQtyFeeModel<DirectionalFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        match (order.maker, order.side) {
            (true, Side::Buy) => {
                self.fees.common_fees.maker_fee * order.exec_qty + self.fees.buyer_fee * amount
            }
            (false, Side::Buy) => {
                self.fees.common_fees.taker_fee * order.exec_qty + self.fees.buyer_fee * amount
            }
            (true, Side::Sell) => {
                self.fees.common_fees.maker_fee * order.exec_qty + self.fees.seller_fee * amount
            }
            (false, Side::Sell) => {
                self.fees.common_fees.taker_fee * order.exec_qty + self.fees.seller_fee * amount
            }
            _ => unreachable!(),
        }
    }
}

/// Flat fee per trade
#[derive(Clone)]
pub struct FlatPerTradeFeeModel<Fees> {
    fees: Fees,
}
impl<Fees> FlatPerTradeFeeModel<Fees> {
    /// Constructs `FlatPerTradeFeeModel`.
    pub fn new(fees: Fees) -> Self {
        Self { fees }
    }
}

impl FeeModel for FlatPerTradeFeeModel<CommonFees> {
    fn amount(&self, order: &Order, _amount: f64) -> f64 {
        if order.maker {
            self.fees.maker_fee
        } else {
            self.fees.taker_fee
        }
    }
}
