#include <atomic>
#include <type_traits>

#pragma pack(push, 1)

struct OrderCommand{
    uint16_t symbol_locate;
    uint64_t order_id;
    uint32_t price;
    uint32_t quantity;
    uint8_t side;
};

struct alignas(64) Slot{
    uint64_t timestamp;
    uint16_t len;
    uint8_t data[54];
};

struct alignas(64) RingHeader{
    std::atomic<uint64_t> magic_num;
    std::atomic<uint64_t> head;
};

#pragma pack(pop)

#pragma pack(push, 1)
struct ExecutionReport {
    uint64_t order_id;      // Match the ID from Rust
    uint32_t last_qty;      // How many shares were just filled
    uint32_t last_price;    // At what price
    uint8_t  exec_type;     // 0 = Fill, 1 = Partial, 2 = Reject
    uint16_t stock_locate;; //stock id
};
#pragma pack(pop)