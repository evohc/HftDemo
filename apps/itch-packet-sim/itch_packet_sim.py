import socket
import struct
import time

UDP_IP = "127.0.0.1"
UDP_PORT = 8080
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

def send_r_packet(locate, symbol):
    buf = bytearray(39)
    buf[0] = ord('R')
    buf[1:3] = struct.pack(">H", locate)
    sym_bytes = symbol.ljust(8).encode('ascii')
    buf[11:19] = sym_bytes
    sock.sendto(buf, (UDP_IP, UDP_PORT))

def send_a_packet(locate, order_id, side, qty, price):
    buf = bytearray(36)
    buf[0] = ord('A')
    buf[1:3] = struct.pack(">H", locate)
    buf[11:19] = struct.pack(">Q", order_id)
    buf[19] = ord(side)
    buf[20:24] = struct.pack(">I", qty) 
    buf[32:36] = struct.pack(">I", price)
    sock.sendto(buf, (UDP_IP, UDP_PORT))

input("Press any key to start itch packet creation...")

print("Mapping AAPL test stock. Waiting 3 secs.")
send_r_packet(10, "AAPL")
time.sleep(3)

print("Sending sell (100 @ 150.00).")
send_a_packet(10, 101, 'S', 100, 15000)
time.sleep(1)

print("Sending large buy (10,000 @ 149.90) to trigger strategy to fire.")
send_a_packet(10, 102, 'B', 10000, 14990)


print("Complete...")