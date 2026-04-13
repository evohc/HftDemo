import socket
import struct

#OrderCommand (19 bytes)
# Python C++    	Size(Bytes)	Name
# H	     uint16_t	2	        symbol_locate
# Q	     uint64_t	8	        order_id
# I	     uint32_t	4	        price
# I	     uint32_t	4	        quantity
# B	     uint8_t	1	        side

ORDER_FMT = "<H Q I I B" 

# ExecutionReport (19 bytes)..no padding needed
# Python C++        Size (Bytes) Name
# Q	     uint64_t	8	         order_id
# I	     uint32_t	4	         fill_qty
# I	     uint32_t	4	         fill_price
# B	     uint8_t	1	         exec_type
# H	     uint16_t	2	         stock_locate

EXEC_FMT = "<Q I I B H" 

def start_exchange():
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(('127.0.0.1', 9001))
    server.listen(1)
    print("Listening on 9001...")

    conn, addr = server.accept()
    print(f"Execution engine connected from {addr}.")

    try:
        while True:
            data = conn.recv(19)
            if not data:
                break
            
            # Unpack the Order
            locate, oid, px, qty, side = struct.unpack(ORDER_FMT, data)
            side_str = "BUY" if side == 0 else "SELL"
            print(f"--> Received: ID {oid} | {side_str} {qty} @ {px}")

            report = struct.pack(EXEC_FMT, oid, qty, px, 0, locate) 
            
            conn.sendall(report)
            print(f"<-- Sent fill confirmation: ID {oid}")

    except Exception as e:
        print(f"Error: {e}")
    finally:
        conn.close()
        server.close()
        print("Shut down.")

if __name__ == "__main__":
    start_exchange()