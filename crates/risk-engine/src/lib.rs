use common::{OrderCommand, OrderSide};

pub enum RiskDecision {
    Pass,
    Fail(String),
}

//Will only do local risk relevant to a individual stock. There should also be a global risk check
// e.g. I don't care if AAPL looks good, we have too much tech stock. Do not buy....
pub struct LocalRisk {
    pub max_qty_per_order: u32,
    pub price_band_percentage: f32,
    pub max_absolute_position: i32,
    pub current_position: i32,
}

impl LocalRisk {
    pub fn new(max_qty: u32, price_band: f32, max_pos: i32) -> Self {
        Self {
            max_qty_per_order: max_qty,
            price_band_percentage: price_band,
            max_absolute_position: max_pos,
            current_position: 0,
        }
    }

    ///Order Quantity Check...verifies the order size is within the maximum
    ///allowed per-message limit
    ///
    ///Inventory Projection...Calculates the projected position by assuming an
    ///immediate 100% fill of the current order. Never exceed global risk limits.
    ///
    ///
    ///Absolute Position Limit...
    ///Uses absolute value to prevent the net position from exceeding the risk threshold on
    ///both the Long (owning too much) and short (selling too much) sides.
    pub fn check(
        &mut self,
        cmd: &OrderCommand,
        best_bid: Option<(u64, u32)>,
        best_ask: Option<(u64, u32)>,
    ) -> RiskDecision {
        let cmd_side = cmd.side;
        let cmd_qty = cmd.quantity;

        if cmd_qty > self.max_qty_per_order {
            return RiskDecision::Fail(format!(
                "Qty {} exceeds limit {}",
                cmd_qty, self.max_qty_per_order
            ));
        }

        let delta = if cmd_side == OrderSide::Buy {
            cmd_qty as i32
        } else {
            -(cmd_qty as i32)
        };
        let projected_pos = self.current_position + delta; //If this order gets filled 100% right now, what would my total inventory look like

        if projected_pos.abs() > self.max_absolute_position {
            return RiskDecision::Fail(format!(
                "Projected pos {} exceeds limit {}",
                projected_pos, self.max_absolute_position
            ));
        }

        match cmd.side {
            OrderSide::Buy => {
                if let Some((ask_price, _)) = best_ask {
                    let limit = ask_price as f32 * (1.0 + self.price_band_percentage);
                    if cmd.price as f32 > limit {
                        return RiskDecision::Fail("Buy price too far above best ask".to_string());
                    }
                }
            }
            OrderSide::Sell => {
                if let Some((bid_price, _)) = best_bid {
                    let limit = bid_price as f32 * (1.0 - self.price_band_percentage);
                    if (cmd.price as f32) < limit {
                        return RiskDecision::Fail("Sell price too far below best bid".to_string());
                    }
                }
            }
        }

        self.current_position = projected_pos;
        RiskDecision::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{OrderCommand, OrderSide};

    fn setup_risk() -> LocalRisk {
        LocalRisk::new(1000, 0.10, 2000)
    }

    #[test]
    fn test_max_qty_breach() {
        let mut risk = setup_risk();
        let cmd = OrderCommand {
            order_id: 1,
            symbol_locate: 10,
            price: 15000,
            quantity: 1500, //breach 1500 > 1000
            side: OrderSide::Buy,
        };

        match risk.check(&cmd, None, None) {
            RiskDecision::Fail(msg) => assert!(msg.contains("exceeds limit")),
            RiskDecision::Pass => panic!("Should have failed max qty check"),
        };
    }

    #[test]
    fn test_position_projection_long_breach() {
        let mut risk = setup_risk();

        //we are already long at 1500 shares, Buy 600 more > 2000 limit
        risk.current_position = 1500;

        let cmd = OrderCommand {
            order_id: 2,
            symbol_locate: 10,
            price: 15000,
            quantity: 600,
            side: OrderSide::Buy,
        };

        match risk.check(&cmd, None, None) {
            RiskDecision::Fail(msg) => assert!(msg.contains("exceeds limit")),
            RiskDecision::Pass => panic!("Should have failed max qty check"),
        };
    }

    #[test]
    fn test_position_projection_short_breach() {
        let mut risk = setup_risk();

        // Short -1800 shares.
        // Sell 300 more. -1800 - 300 = -2100. abs(-2100) = 2100 (> 2000 limit)
        risk.current_position = -1800;

        let cmd = OrderCommand {
            order_id: 3,
            symbol_locate: 10,
            price: 15000,
            quantity: 300,
            side: OrderSide::Sell,
        };

        match risk.check(&cmd, None, None) {
            RiskDecision::Fail(msg) => assert!(msg.contains("exceeds limit")),
            RiskDecision::Pass => panic!("Should have failed short position projection"),
        }
    }

    #[test]
    fn test_buy_price_band_breach() {
        let mut risk = setup_risk();

        // Best ask is 100.00 (10000). 10% band means max buy price is 110.00 (11000).
        let best_ask = Some((10000, 100));

        let cmd = OrderCommand {
            order_id: 4,
            symbol_locate: 10,
            price: 11500, // 115.00 is > 110.00
            quantity: 100,
            side: OrderSide::Buy,
        };

        match risk.check(&cmd, None, best_ask) {
            RiskDecision::Fail(msg) => assert!(msg.contains("too far above")),
            RiskDecision::Pass => panic!("Should have failed buy price band"),
        }
    }

    #[test]
    fn test_successful_pass_updates_position() {
        let mut risk = setup_risk();
        assert_eq!(risk.current_position, 0);

        let cmd = OrderCommand {
            order_id: 5,
            symbol_locate: 10,
            price: 10000,
            quantity: 500,
            side: OrderSide::Buy,
        };

        let result = risk.check(&cmd, None, Some((10000, 100)));

        // Verify it passed
        assert!(matches!(result, RiskDecision::Pass));
        // Verify the Risk Engine committed the projected position
        assert_eq!(risk.current_position, 500);
    }

    #[test]
    fn test_sell_price_band_breach() {
        let mut risk = LocalRisk::new(1000, 0.10, 5000);

        // Best Bid is 100.00
        // 10% band means we shouldn't sell for less than 90.00
        let best_bid = Some((10000, 500));

        let cmd = OrderCommand {
            order_id: 10,
            symbol_locate: 10,
            price: 8500, // 15% drop,..should be rejected
            quantity: 100,
            side: OrderSide::Sell,
        };

        match risk.check(&cmd, best_bid, None) {
            RiskDecision::Fail(msg) => {
                assert!(msg.contains("Sell price too far below"));
                println!("Success: Rejected toxic sell price.");
            }
            RiskDecision::Pass => panic!("Risk Engine should have blocked a 15% price drop!"),
        }

        // Test a safe sell within 10%
        let safe_cmd = OrderCommand {
            order_id: 11,
            symbol_locate: 10,
            price: 9500, // 95.00: Within the 10% band (90.00 limit)
            quantity: 100,
            side: OrderSide::Sell,
        };

        match risk.check(&safe_cmd, best_bid, None) {
            RiskDecision::Pass => println!("Success: Allowed safe sell price."),
            RiskDecision::Fail(e) => panic!("Should have passed $95.00 sell. Error: {}", e),
        }
    }
}
