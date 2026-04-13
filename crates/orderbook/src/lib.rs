use common::Side;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy)]
pub struct Order {
    pub price: u64,
    pub quantity: u32,
    pub side: Side,
}

/*
L3 (The Micro View)
Tracks who is in the line and when they got there.
e.g., Order 4592 wants 100 shares at $150, Order 593 wants 200 shares at 150)

L2 (The Macro View)
Tracks how big the line is at a specific price.
e.g., There is a total of 300 shares waiting to be bought at $150.
*/

pub struct L3OrderBook {
    pub orders: HashMap<u64, Order>,
    //Key:Price, Value: Total Quantity at that price.
    pub bids: BTreeMap<u64, u32>, //Best Bid: This is the last (highest) price.
    pub asks: BTreeMap<u64, u32>, //Best Ask: It's the first (lowest) price.
}

impl L3OrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn add_order(&mut self, id: u64, side: Side, price: u64, quantity: u32) {
        let new_order = Order {
            price,
            quantity,
            side,
        };

        self.orders.insert(id, new_order);

        let book_side = match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        *book_side.entry(price).or_insert(0) += quantity;
    }

    /*Find order num in the HashMap to get price and type.
    Subtract the cancelled shares from the specific order.
    In BTreeMap for price, subtract the shares from the public total.
    If the specific Order hits 0 shares, delete from HashMap.
    If the price level hits 0 shares, delete fromBTreeMap.*/

    pub fn cancel_order(&mut self, id: u64, quantity_to_cancel: u32) {
        if let Some(order) = self.orders.get_mut(&id) {
            let price = order.price;
            let side = order.side;

            order.quantity -= quantity_to_cancel;
            let new_order_quantity = order.quantity;

            let book_side = match side {
                Side::Sell => &mut self.asks,
                Side::Buy => &mut self.bids,
            };

            if let Some(total_quantity) = book_side.get_mut(&price) {
                *total_quantity -= quantity_to_cancel;

                if *total_quantity == 0 {
                    book_side.remove(&price);
                }
            }

            if new_order_quantity == 0 {
                self.orders.remove(&id);
            }
        }
    }

    // Returns (Price, Total Quantity) for the best buyer
    pub fn get_best_bid(&self) -> Option<(u64, u32)> {
        //.next_back() gets the highest price level, iterator gives you references to the data inside the map,controls memory
        // and returns Option<(&u64, &u32)>, .map() ->transform value into something else...// Extract the values from the pointers
        // return them as a pair
        //self.bids.iter().next_back().map(|(&p, &q)| (p, q))

        //this is easier
        let mut iterator = self.bids.iter();

        let best_bid_ref = iterator.next_back();

        let result = best_bid_ref.map(|(&p, &q)| {
            let pair = (p, q);
            return pair;
        });

        return result;
    }

    // Returns (Price, Total Quantity) for the best seller
    pub fn get_best_ask(&self) -> Option<(u64, u32)> {
        self.asks.iter().next().map(|(&p, &q)| (p, q))
    }

    pub fn spread(&self) -> Option<u64> {
        if let (Some((bid, _)), Some((ask, _))) = (self.get_best_bid(), self.get_best_ask()) {
            return Some(ask.saturating_sub(bid));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_order_and_aggreation() {
        let mut book = L3OrderBook::new();

        //add first bid.
        book.add_order(101, Side::Buy, 150, 10);

        let order = book.orders.get(&101).expect("Order 101 should exist");
        assert_eq!(order.quantity, 10);
        assert_eq!(order.price, 150);

        //check price exists in map
        assert_eq!(*book.bids.get(&150).unwrap(), 10);

        //Add a second price at same price of 150 with quantity 20
        book.add_order(102, Side::Buy, 150, 20);
        assert_eq!(*book.bids.get(&150).unwrap(), 30);

        //Individual orders still keep their own size
        assert_eq!(book.orders.get(&101).unwrap().quantity, 10);
        assert_eq!(book.orders.get(&102).unwrap().quantity, 20);
    }

    #[test]
    fn test_partial_cancel() {
        let mut book = L3OrderBook::new();
        book.add_order(101, Side::Buy, 150, 50);

        book.cancel_order(101, 25);

        //check remaining amount
        assert_eq!(*book.bids.get(&150).unwrap(), 25);

        assert_eq!(book.orders.get(&101).unwrap().quantity, 25);
    }

    #[test]
    fn test_full_cancel() {
        let mut book = L3OrderBook::new();
        book.add_order(101, Side::Buy, 150, 50);

        book.cancel_order(101, 50);

        //check 0 quantity is gone.
        assert!(
            book.bids.get(&150).is_none(),
            "Order 101 should be deleted."
        );

        assert!(
            book.orders.get(&101).is_none(),
            "Price level 150 should be deleted."
        );
    }

    #[test]
    fn test_mixed_cleanup() {
        // Test removing one order while another remains at the same price
        let mut book: L3OrderBook = L3OrderBook::new();

        book.add_order(101, Side::Buy, 150, 50);
        book.add_order(102, Side::Buy, 150, 25);

        assert_eq!(*book.bids.get(&150).unwrap(), 75);

        book.cancel_order(101, 50);

        assert!(book.orders.get(&101).is_none());
        assert!(book.orders.get(&102).is_some());

        assert_eq!(*book.bids.get(&150).unwrap(), 25);
    }

    #[test]
    fn test_best_ask_bid_spread() {
        let mut book: L3OrderBook = L3OrderBook::new();

        book.add_order(101, Side::Buy, 100, 50);
        book.add_order(102, Side::Buy, 105, 25);
        book.add_order(103, Side::Buy, 95, 33);

        book.add_order(104, Side::Sell, 110, 21);
        book.add_order(105, Side::Sell, 160, 25);
        book.add_order(106, Side::Sell, 225, 25);

        assert_eq!(book.get_best_bid().unwrap().0, 105);
        assert_eq!(book.get_best_ask().unwrap().0, 110);

        assert_eq!(book.spread().unwrap(), 5);
    }
}
