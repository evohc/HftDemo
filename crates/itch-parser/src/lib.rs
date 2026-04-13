use common::Side;

pub enum ItchMessage {
    //Message type 'A' : new order
    AddOrder {
        stock_locate: u16,
        order_id: u64,
        side: Side,
        quantity: u32,
        price: u64,
    },
    //Message type 'E' or 'X' : Existing order is reduced/removed
    ModifyOrder {
        stock_locate: u16,
        order_id: u64,
        quantity_to_remove: u32,
    },
    //Message type 'P' Off book trade. No order Id as we cant do anthing with it.
    Trade {
        stock_locate: u16,
        price: u64,
        quantity: u32,
        match_number: u64,
        bid: Side,
    },
    //Message type 'D' delete order
    DeleteOrder {
        stock_locate: u16,
        order_id: u64,
    },
    //Message type 'E' order excuted
    OrderExecuted {
        stock_locate: u16,
        order_id: u64,
        quantity: u32,
        match_number: u64,
    },
    //Message type 'C' order excuted with price
    OrderExecutedWithPrice {
        stock_locate: u16,
        order_id: u64,
        quantity: u32,
        price: u64,
        match_number: u64,
    },

    //Message Type 'R' stock directory listing for day
    StockDirectory {
        stock_locate: u16,
        symbol: String,
    },
}

pub fn parse_itch_message(data: &[u8]) -> Option<ItchMessage> {
    //first byte is message type
    let message_type = data[0] as char;
    let stock_locate = u16::from_be_bytes(data[1..3].try_into().ok()?); //same for all messages

    match message_type {
        /*
        STOCK DIRECTORY:
            Field Name	          Offset  Len
            Stock Directory Msg   0	      1
            Stock Locate	      1	      2   The unique ID assigned for the day.
            Tracking Num	      3	      2
            Timestamp	          5	      6
            Stock       	      11	  8   The actual symbol (e.g., "AAPL    ")
            ....
        */
        'R' => {
            let symbol = String::from_utf8_lossy(&data[11..19]).trim().to_string(); //heap allocation here this is pre trading but fix it.

            Some(ItchMessage::StockDirectory {
                stock_locate,
                symbol,
            })
        }

        /*
        ADD ORDER:
            Field Name	    Offset	Len
            Message Type	0	    1
            Stock Locate	1	    2
            Tracking Num	3	    2
            Timestamp	    5	    6
            Order Ref Num	11	    8
            Buy/Sell 	    19	    1
            Shares	        20	    4
            Stock	        24	    8
            Price	        32	    4
        */
        'A' => {
            let order_id = u64::from_be_bytes(data[11..19].try_into().unwrap());

            let side = match data[19] as char {
                'B' => Side::Buy,
                'S' => Side::Sell,
                _ => return None,
            };

            let quantity = u32::from_be_bytes(data[20..24].try_into().unwrap());

            let price = u32::from_be_bytes(data[32..36].try_into().unwrap()) as u64;

            Some(ItchMessage::AddOrder {
                stock_locate,
                order_id,
                side,
                quantity,
                price,
            })
        }

        /*
        CANCEL ORDER (partial)
            Field Name 	    Offset	 Len
            Message Type	0	     1
            Stock Locate	1	     2
            Tracking Num	3	     2
            Timestamp	    5	     6
            Order Ref Num	11	     8
            Canceled Shares	19	     4
        */
        'X' => {
            let order_id = u64::from_be_bytes(data[11..19].try_into().unwrap());
            let quantity_to_remove = u32::from_be_bytes(data[19..23].try_into().unwrap());

            Some(ItchMessage::ModifyOrder {
                stock_locate,
                order_id,
                quantity_to_remove,
            })
        }

        /*
        DELETE ORDER
            Field Name 	    Offset	Len
            Message Type	0	    1
            Stock Locate	1	    2
            Tracking Num	3	    2
            Timestamp	    5	    6
            Order Ref Num	11	    8
         */
        'D' => {
            let order_id = u64::from_be_bytes(data[11..19].try_into().unwrap());

            Some(ItchMessage::DeleteOrder {
                stock_locate,
                order_id,
            })
        }

        /*
        TRADE ORDER
            Field Name 	    Offset	Len
            Message Type	0	    1
            Stock Locate	1	    2
            Tracking Num	3	    2
            Timestamp	    5	    6
            Order Ref Num	11	    8
            Buy/Sell    	19	    1
            Executed Shares	20	    4
            Stock	        24	    8
            Non-Cross Price 32	    4
            Match Number	36	    8

        Note Trade order doesnt contain
        */
        'P' => {
            let quantity = u32::from_be_bytes(data[20..24].try_into().unwrap());
            let price: u64 = u32::from_be_bytes(data[32..36].try_into().unwrap()) as u64;
            let match_number = u64::from_be_bytes(data[36..44].try_into().unwrap());

            let bid = match data[19] as char {
                'B' => Side::Buy,
                'S' => Side::Sell,
                _ => return None,
            };

            Some(ItchMessage::Trade {
                stock_locate,
                price,
                quantity,
                match_number,
                bid,
            })
        }
        /*
        ORDER EXECUTED
            Field Name	   Offset	Len
            Message Type	0	    1
            Stock Locate	1	    2
            Tracking Num	3	    2
            Timestamp	    5	    6
            Order Ref Num	11	    8
            Executed Shares	19	    4
            Match Number	23	    8
        */
        'E' => {
            let order_id = u64::from_be_bytes(data[11..19].try_into().unwrap());
            let quantity = u32::from_be_bytes(data[19..23].try_into().unwrap());
            let match_number = u64::from_be_bytes(data[23..31].try_into().unwrap());

            Some(ItchMessage::OrderExecuted {
                stock_locate,
                order_id,
                quantity,
                match_number,
            })
        }

        /*
        ORDER EXECUTED PRICE
            Field Name	         Offset	Len
            Message Type	     0	    1
            Stock Locate	     1	    2
            Tracking Num	     3	    2
            Timestamp	         5	    6
            Order Ref Num	    11	    8
            Executed Shares	    19	    4
            Match Number	    23	    8
            Printable           31      1
            Execution Price     32      4
        */
        'C' => {
            let order_id = u64::from_be_bytes(data[11..19].try_into().unwrap());
            let quantity = u32::from_be_bytes(data[19..23].try_into().unwrap());
            let match_number = u64::from_be_bytes(data[23..31].try_into().unwrap());
            let price: u64 = u32::from_be_bytes(data[32..36].try_into().unwrap()) as u64;

            Some(ItchMessage::OrderExecutedWithPrice {
                stock_locate,
                order_id,
                quantity,
                price,
                match_number,
            })
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_order() {
        let mut packet = [0u8; 36];

        packet[0] = b'A';

        let stock_locate_bytes = 1111u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let order_id_bytes = 1000u64.to_be_bytes();
        packet[11..19].copy_from_slice(&order_id_bytes);

        packet[19] = b'B';
        let qty_bytes = 50u32.to_be_bytes();
        packet[20..24].copy_from_slice(&qty_bytes);

        let price_bytes = 1_500_000u32.to_be_bytes();
        packet[32..36].copy_from_slice(&price_bytes);

        let result = parse_itch_message(&packet).expect("Should parse 'A' message");

        if let ItchMessage::AddOrder {
            stock_locate,
            order_id,
            side,
            quantity,
            price,
        } = result
        {
            assert_eq!(stock_locate, 1111);
            assert_eq!(order_id, 1000);
            assert_eq!(side, Side::Buy);
            assert_eq!(quantity, 50);
            assert_eq!(price, 1_500_000);
        } else {
            panic!("Parsed message was not AddOrder");
        }
    }

    #[test]
    fn test_invalid_packet() {
        let mut packet = [0u8; 36];
        packet[0] = b'A';

        packet[19] = b'Z';

        let result = parse_itch_message(&packet);
        assert!(result.is_none());
    }

    #[test]
    fn test_cancel_order() {
        let mut packet = [0u8; 23];
        packet[0] = b'X';

        let stock_locate_bytes = 42u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let order_id_bytes = 2000u64.to_be_bytes();
        packet[11..19].copy_from_slice(&order_id_bytes);

        let qty_bytes = 100u32.to_be_bytes();
        packet[19..23].copy_from_slice(&qty_bytes);

        let result = parse_itch_message(&packet).expect("Should parse 'X' message");

        if let ItchMessage::ModifyOrder {
            stock_locate,
            order_id,
            quantity_to_remove,
        } = result
        {
            assert_eq!(stock_locate, 42);
            assert_eq!(order_id, 2000);
            assert_eq!(quantity_to_remove, 100);
        }
    }

    #[test]
    fn test_delete_order() {
        let mut packet = [0u8; 19];
        packet[0] = b'D';

        let stock_locate_bytes = 82u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let order_id_bytes = 3000u64.to_be_bytes();
        packet[11..19].copy_from_slice(&order_id_bytes);

        let result = parse_itch_message(&packet).expect("Should parse 'D' message");

        if let ItchMessage::DeleteOrder {
            stock_locate,
            order_id,
        } = result
        {
            assert_eq!(stock_locate, 82);
            assert_eq!(order_id, 3000);
        }
    }
    #[test]
    fn test_trade_order() {
        let mut packet = [0u8; 44];
        packet[0] = b'P';

        let stock_locate_bytes = 99u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let qty_bytes = 200u32.to_be_bytes();
        packet[20..24].copy_from_slice(&qty_bytes);

        let price_bytes = 2_500_000u32.to_be_bytes();
        packet[32..36].copy_from_slice(&price_bytes);

        let match_number = 99999u64.to_be_bytes();
        packet[36..44].copy_from_slice(&match_number);

        packet[19] = b'B';

        let result = parse_itch_message(&packet).expect("Should parse 'P' message");

        if let ItchMessage::Trade {
            stock_locate,
            price,
            quantity,
            match_number,
            bid,
        } = result
        {
            assert_eq!(stock_locate, 99);
            assert_eq!(price, 2_500_000);
            assert_eq!(quantity, 200);
            assert_eq!(match_number, 99999);
            assert_eq!(bid, Side::Buy);
        }
    }
    #[test]
    fn test_order_executed() {
        let mut packet = [0u8; 31];
        packet[0] = b'E';

        let stock_locate_bytes = 12u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let order_id_bytes = 4000u64.to_be_bytes();
        packet[11..19].copy_from_slice(&order_id_bytes);

        let qty_bytes = 300u32.to_be_bytes();
        packet[19..23].copy_from_slice(&qty_bytes);

        let match_number = 88888u64.to_be_bytes();
        packet[23..31].copy_from_slice(&match_number);

        let result = parse_itch_message(&packet).expect("Should parse 'E' message");

        if let ItchMessage::OrderExecuted {
            stock_locate,
            order_id,
            quantity,
            match_number,
        } = result
        {
            assert_eq!(stock_locate, 12);
            assert_eq!(order_id, 4000);
            assert_eq!(quantity, 300);
            assert_eq!(match_number, 88888);
        }
    }
    #[test]
    fn test_order_executed_price() {
        let mut packet = [0u8; 36];
        packet[0] = b'C';

        let stock_locate_bytes = 52u16.to_be_bytes();
        packet[1..3].copy_from_slice(&stock_locate_bytes);

        let order_id_bytes = 4000u64.to_be_bytes();
        packet[11..19].copy_from_slice(&order_id_bytes);

        let qty_bytes = 300u32.to_be_bytes();
        packet[19..23].copy_from_slice(&qty_bytes);

        let match_number = 88888u64.to_be_bytes();
        packet[23..31].copy_from_slice(&match_number);

        let price_bytes = 3_500_000u32.to_be_bytes();
        packet[32..36].copy_from_slice(&price_bytes);

        let result = parse_itch_message(&packet).expect("Should parse 'C' message");

        if let ItchMessage::OrderExecutedWithPrice {
            stock_locate,
            order_id,
            quantity,
            price,
            match_number,
        } = result
        {
            assert_eq!(stock_locate, 52);
            assert_eq!(order_id, 4000);
            assert_eq!(quantity, 300);
            assert_eq!(match_number, 88888);
            assert_eq!(price, 3_500_000);
        }
    }
}
