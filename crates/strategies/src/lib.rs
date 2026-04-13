//Note : The Order Book is a snapshot of what people are proposing to do,
//the Trade Ticker is a stream of what is actually done.

/*
1. Order Book
    The Bid (Buyers): A queue of people waiting to buy. They want the lowest
    price possible. The person at the front of the line is the best bid (e.g. 150.00).

    The Ask (Sellers): A queue of people waiting to sell. They want the
    highest price possible. The person at the front of the line is the best ask (e.g. 150.10).
    The Spread: The "no-man's land" between the two lines. In this case, it is 10 cents.

2. Passive(maker) vs Aggressive(taker)

    Passive (Provide Liquidity): You join a line. You place an order and wait.
        Example: You join the Buy line at 150.01. You are now the "Best Bid," but you haven't bought
        anything yet. You are MAKING the market.

    Aggressive (Take Liquidity): You "hit" someone already in line.
        Example: You don't want to wait; you buy from the seller at 150.10 immediately. You are
        TAKING liquidity away.

 Making the Decision (The Strategy)

************Volume Imbalance (The "Weight" of the Market)************

 Bullish Example (The Buy Wall)
    Best Bid: 150.00 with 10,000 shares (1.5 Million USD).
    Best Ask: 150.10 with 100 shares (15,010 USD).

    If a random seller shows up and dumps 500 shares, the Bid price stays
    at 150.00 (because 9,500 shares are still left). But if a buyer shows up
    and buys 200 shares, the Ask price at 150.10 vanishes instantly.
    The Decision: BUY. The "resistance" to the upside is tiny, while the
    "support" on the downside is a fortress.

    The Ask price at 150.10 vanishes instantly.
    The Scenario: A buyer (could be another HFT, a bank, or a retail trader) decides they want to buy 200 shares right now.
    They "take" the 100 shares available at 150.10. Those are now gone
    They still need 100 more shares. They have to look at the next person in the Sell Line.
    The next seller might be asking for 150.11 or 150.15.
    The price of 150.10 is no longer the "Best Ask." It has "vanished" because the
    inventory was sold out. The "price" of the stock has effectively moved up to 150.11

    Resistance to the upside is tiny... support on the downside is a fortress.
    This describes the effort required to move the price in either direction.

    Think of the "Ask" side (150.10) as a Thin Glass Wall.
    It only takes a small amount of "buying power" (101 shares) to shatter that wall and move the price higher.
    Because the wall is so weak, the price can "jump" up very quickly with very little money.
    Think of the "Bid" side (150.00) as a Thick Concrete Wall.
    It would take a massive "selling hammer" (over 10,000 shares) to break that level.
    Even if several people sell 1,000 shares each, the price stays exactly at 150.00. It is "supported."

    The Decision: BUY
    "If I buy at 150.10 right now, I am protected. Even if the market turns slightly sour,
    that 10,000-share fortress at 150.00 will catch me. But if the market stays bullish,
    the tiny 100-share glass wall will break, and I'll be in profit instantly as the price 
    moves to 150.11, 150.12, etc."

 Bearish Example (The Sell Wall)
    Best Bid: 150.00 with 200 shares (30,000 USD).
    Best Ask: 150.10 with 50,000 shares (7.5 Million USD).
    The Logic: The buyers are weak. One small sell order will "break" the 150.00 level.
    Meanwhile, for the price to go up, buyers would have to spend 7.5 million just to get past 150.10.
    The Decision: SELL. The path of least resistance is down.

    The Best Bid (150.00) only has 200 shares waiting.
    The "Thin Ice": Imagine the price is standing on a very thin sheet of ice. Only 200 shares are holding it up.
    Breaking the Level: If a single trader decides to sell just 201 shares, the 150.00 price "breaks."
    The buyers at that price are completely wiped out.
    The Result: The "Best Bid" now drops to whatever the next person in line is willing to pay—maybe
    149.95 or 149.90. The price falls instantly because there was no "crowd" to catch it.

    On the other side of the tug-of-war, the Sellers have built a Giant Brick Wall at 150.10 with 50,000 shares.
    The Implication: For the stock price to move even one cent higher (to 150.11), the world must
    first buy every single one of those 50,000 shares.
    To a small HFT algorithm, 7.5 million is a huge amount of money. Unless a massive bank or a "
    Whale" shows up to buy everything, that price is not going anywhere. It is a "Hard Ceiling.

    The Decision: SELL
    The upside is blocked by a 7.5 million wall. The downside is only protected by a tiny 
    30,000 bid. If I sell now at 150.00, I am getting out before the 'Ice' breaks. There is 
    almost zero chance the price will accidentally go up past that giant wall, but a very high 
    chance it will collapse through that tiny floor.

    You aren't predicting the future based on news or earnings; you are simply looking at the physical 
    weight of the orders. If the Sell side is 250  times heavier than the Buy side (50,000 vs 200), 
    the price is almost "magnetically" pulled downward.

    In one line -- if there are way more sellers sell, way more buyers buy.

    Why "Way More Sellers" = Sell
    If there are 50,000 shares for sale and only 200 shares wanted:
    The sellers realize they can't all sell at the current price.
    The "smart" sellers will start lowering their price to "jump the line" and grab opportunity to exit  before someone else does.
    This causes a race to the bottom, which drives the price down instantly.

    Why "Way More Buyers" = Buy, It’s a bidding war.
    If there are 10,000 shares wanted and only 100 for sale:

    The buyers realize the supply is almost gone.
    The "smart" buyers will pay a little more (150.11 instead of 150.10) just to make sure they get their shares.
    This causes a race to the top, driving the price up.

    The Thought Process: "I think this stock is actually worth 152.00. Buying it for 150.11 is
    still a bargain, even if it's 'more expensive' than the last trade. If I wait for 150.00, I'll be left behind."

************Midpoint Drift (The "Speed" of the Market)************
Bullish Example (Drift Up)
    Time 1: Bid is 150.00, Ask is 150.10.
        Midpoint: (150.00 + 150.10) / 2 = 150.05
    Time 2: A new buyer aggressively joins at 150.08 (they don't want to wait at 150.00).
        New Midpoint: (150.08 + 150.10) / 2 = 150.09
    The Result: The Midpoint drifted +4 cents in a split second.
    The Decision: BUY. The "Fair Value" is being pulled upward by aggressive bidders.

Bearish Example (Drift Down)
    Time 1: Bid is 150.00, Ask is 150.10.
        Midpoint: 150.05
    Time 2: A seller gets nervous and lowers their asking price from 150.10 down to 150.02.
        New Midpoint: (150.00 + 150.02) / 2 = {150.01}
    The Result: The Midpoint drifted -4 cents.
    The Decision: SELL. Sellers are "leaning" into the buyers, pushing the fair value down.
*/

use common::{AggressorSide, OrderCommand, OrderSide};

pub trait Strategy {
    fn on_trade(&mut self, price: u64, quantity: u32, side: AggressorSide) -> Option<OrderCommand>;
    fn on_book_update(
        &mut self,
        best_bid: Option<(u64, u32)>,
        best_ask: Option<(u64, u32)>,
    ) -> Option<OrderCommand>;
}

#[derive(Debug, Copy, Clone)]
pub struct SimpleMidPoint {
    pub mid_price: f64,
    pub prev_mid_price: f64,
    pub trade_count: u64,
    pub position: i32,
    pub next_order_id: u64,
    pub imbalance_threshold: f64,
}

impl SimpleMidPoint {
    pub fn new(threshold: f64) -> Self {
        Self {
            mid_price: 0.0,
            prev_mid_price: 0.0,
            trade_count: 0,
            position: 0,
            next_order_id: 1,
            imbalance_threshold: threshold,
        }
    }

    fn generate_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }
}

//Use volume imbalance and midpoint drift.
//only send "Immediate or Cancel" and we wont use exchange ID (only figured out what exhange ID was late in process)
impl Strategy for SimpleMidPoint {
    fn on_book_update(
        &mut self,
        //(Price, Quantity, Exchange)
        best_bid: Option<(u64, u32)>,
        best_ask: Option<(u64, u32)>,
    ) -> Option<OrderCommand> {
        if let (Some((bid_price, bid_qty)), Some((ask_price, ask_qty))) = (best_bid, best_ask) {
            self.prev_mid_price = self.mid_price;
            //fair value
            self.mid_price = (bid_price + ask_price) as f64 / 2.0;

            let total_qty = (bid_qty + ask_qty) as f64;

            if total_qty == 0.0 {
                //no volume / no signal
                return None;
            }

            let imbalance = (bid_qty as f64) / total_qty; //What percentage of total volume belongs to the buyers

            //imbalance represents the buyer's share of the volume:
            //buy when the buyers own more than 80% of the volume
            if imbalance > self.imbalance_threshold
                && self.mid_price > self.prev_mid_price
                && self.position <= 5000
            //// Strategy-level cap matching Risk Engine
            //Only buy if we don't already own stock...also use a max position.
            {
                return Some(OrderCommand {
                    symbol_locate: 0,
                    order_id: self.generate_id(),
                    price: ask_price as u32,
                    quantity: 100,
                    side: OrderSide::Buy,
                });
            }

            //sell when the sellers own more than 80% of the volume
            //We are measuring buyer strenght. If sellers own 80%, that means buyers only own 20%
            if imbalance < (1.0 - self.imbalance_threshold)
                && self.mid_price < self.prev_mid_price
                && self.position > -5000
            //Long > position 0 need to sell NOW to protect my profit (or stop my loss) before the price drops further
            //OR
            //Neutral - don't own any shares, but the market looks  weak.
            //Short stock to make money on the way down.
            {
                return Some(OrderCommand {
                    symbol_locate: 0,
                    order_id: self.generate_id(),
                    price: bid_price as u32,
                    quantity: 100,
                    side: OrderSide::Sell,
                });
            }

            /*
            0.0 — 0.2: Sell Zone (Sellers have > 80% weight)
            0.2 — 0.8: No trade (The market is too balanced/noisy)
            0.8 - 1.0: Buy zone (Buyers have > 80% weight)
            ...if there are way more sellers sell, way more buyers buy.
            */
        }
        None
    }

    fn on_trade(
        &mut self,
        _price: u64,
        quantity: u32,
        side: AggressorSide,
    ) -> Option<OrderCommand> {
        self.trade_count += 1;

        //assume if we sent an order, it got filled
        match side {
            AggressorSide::Buyer => self.position += quantity as i32, //we are buyer position is up
            AggressorSide::Seller => self.position -= quantity as i32, //we are seller position is down
        }

        None
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_buy_trigger_on_imbalance() {
        let mut strategy = SimpleMidPoint::new(0.8);

        //10,000 buy vs 100 Sell. Price goes from 150.05->150.06
        //strategy.mid_price = 15005.0; //will fail as it thinks price is same use older value
        strategy.mid_price = 14990.0;

        let bid = Some((15000, 10000));
        let ask = Some((15010, 100));

        let result = strategy.on_book_update(bid, ask);
        let order = result.unwrap();

        let order_side = order.side;
        let order_price = order.price;

        assert_eq!(order_side, OrderSide::Buy);
        assert_eq!(order_price, 15010);
    }

    #[test]
    fn test_position_limit_prevents_overbuy() {
        let mut strategy = SimpleMidPoint::new(0.8);
        strategy.position = 5100;

        let bid = Some((15000, 10000));
        let ask = Some((15010, 100));

        let result = strategy.on_book_update(bid, ask);

        // No order because position is already >= 0 i.e. we own shares.
        assert!(result.is_none());
    }

    #[test]
    fn test_zero_volume_safety() {
        let mut strategy = SimpleMidPoint::new(0.8);
        // Both sides are 0
        let bid = Some((15000, 0));
        let ask = Some((15010, 0));

        let result = strategy.on_book_update(bid, ask);

        // Should return None, not crash/panic!
        assert!(result.is_none());
    }

    #[test]
    fn test_short_trigger_on_sell_imbalance() {
        let mut strategy = SimpleMidPoint::new(0.8);
        strategy.mid_price = 15005.0;

        let bid = Some((14990, 100));
        let ask = Some((15000, 10000));

        let result = strategy.on_book_update(bid, ask);

        assert!(result.is_some());
        let order = result.unwrap();

        let order_side = order.side;
        let order_price = order.price;

        assert_eq!(order_side, OrderSide::Sell);
        assert_eq!(order_price, 14990);
    }

    #[test]
    fn test_neutral_market_does_nothing() {
        let mut strategy = SimpleMidPoint::new(0.8);
        strategy.mid_price = 14000.0;

        // 50/50 Imbalance
        let bid = Some((15000, 5000));
        let ask = Some((15010, 5000));

        let result = strategy.on_book_update(bid, ask);

        assert!(result.is_none());
    }
}
