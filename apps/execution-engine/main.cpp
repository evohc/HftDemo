#include "helper.hpp"
#include <cstring>
#include <unistd.h>
#include <thread>
#include <x86intrin.h>
#include <pthread.h>
#include <sched.h>


void processExecution(ExecutionReport* execReport, ShmDetails *producerShmDetails){

    size_t trackIdx = execReport->order_id % MAX_ORDER_TRACK;
    uint64_t sentTsc = sendTimeStamps[trackIdx];
    uint64_t cycles = __rdtsc() - sentTsc;

    double microsTime = cycles / 3000.0;

    printf("[Received Exchange confirmation] ID: %lu | Qty: %u | Px: %u | Status: %u  | Latency: %ld cycles (%.2f us)\n",
           execReport->order_id,
           execReport->last_qty,
           execReport->last_price,
           execReport->exec_type,
           cycles,
           microsTime);

    const uint64_t inboundMask = (producerShmDetails->shmSize / 64) - 1;

    auto* inboundSlots = reinterpret_cast<Slot*>(static_cast<uint8_t*>(producerShmDetails->pshmMemMmap) + 64);
    size_t index = producerShmDetails->read_write_index & inboundMask;
    Slot& s = inboundSlots[index];
    s.timestamp = 0;
    s.len = sizeof(ExecutionReport);

    std::memcpy(s.data, execReport, sizeof(ExecutionReport));

    producerShmDetails->read_write_index ++;

    producerShmDetails->header->head.store(producerShmDetails->read_write_index, std::memory_order_release);
}

int main() {    

    std::cout << "Starting execution engine..." << std::endl;

    cpu_set_t cpuSet;
    CPU_ZERO(&cpuSet);
    CPU_SET(3, &cpuSet);

    const auto res = pthread_setaffinity_np(pthread_self(), sizeof(cpu_set_t), &cpuSet);
    if(res){
        std::cerr << "Error calling pthread_setaffinity_np" << std::endl;
    }else{
        std::cout << "Pinned to Core 3." << std::endl;
    }

    auto consumerSmhDetails = setupConsumerSharedMemory(); 
    auto producerSmhDetails = setupProducerSharedMemory();

    const auto sock = setUpNetwork();

    auto* slots = reinterpret_cast<Slot*>(static_cast<uint8_t*>(consumerSmhDetails.pshmMemMmap) + 64);
    const uint64_t mask = (consumerSmhDetails.shmSize / 64) - 1; // power-of-two mask for 64MB ring

    std::cout << "Spinning...Press Ctrl+C to stop." << std::endl;

    uint8_t exhangeResponse[1024]; 
    size_t writeCursor = 0;
    const size_t executionReportSize = sizeof(ExecutionReport); // e.g., 24 bytes

    //one single loop to check is there a new order from Rust and is there a message from exchange.

    /*
    TCP is a continous stream of bytes. Unlike UDP which preserves message boundaries if 
    you send 20bytes you get 20 bytes.  Because TCP is a hose of data, there is 3 possibilities...
    Happy path - exchange send one 24 bytes ExecutionReport.  We are good, reads it and move on
    Batching - exchange sends three 24 bytes ExecutionReport. To be efficient NIC/Kernel groups 
    all 72 bytes - revc needs to handle this.
    Fragmentation - exchange send 24 bytes and it arrives in parts.  recv returns 10 bytes...y
    */
    while(true){
        uint64_t current_head = consumerSmhDetails.header->head.load(std::memory_order_acquire);

        if(consumerSmhDetails.read_write_index < current_head){
            //find slot using mask.
            size_t index = consumerSmhDetails.read_write_index & mask;
            Slot& s = slots[index];

            const auto* cmd = reinterpret_cast<OrderCommand*>(s.data);

            printf("[Order Received from Rust, send to exchange] Seq: %lu | Sym: %u | ID: %lu  | Side: %s | Qty: %u | Px: %u\n",
               consumerSmhDetails.read_write_index,
               cmd->symbol_locate,
               cmd->order_id,
               (cmd->side == 0 ? "BUY" : "SELL"),
               cmd->quantity,
               cmd->price);

            size_t track_idx = cmd->order_id % MAX_ORDER_TRACK;
            sendTimeStamps[track_idx] = __rdtsc();

            const auto sent = send(sock, cmd, sizeof(OrderCommand), MSG_NOSIGNAL) ; //MSG_NOSIGNAL if exchange messes up Linux kernel sends SIGPIPE and can kill program, fail and retry here
            
            // in a real system you would need a full reconnect state machine e.g.tell trading logic to stop, set back up connection
            // and check with exchange what was last succesful checkpoint e.g. what last order did you get from me
            if(sent < 0){
                if (errno == EAGAIN || errno == EWOULDBLOCK){
                    std::cerr << "Network buffer full, possible latency spike." << std::endl;
                    //what to do here...
                    continue;
                }else if (errno == EPIPE  || errno == ECONNRESET){
                    std::cerr << "Connection to Exchange lost. Exit..." << std::endl;
                    exit(-1);
                }
                else{
                    perror("Send failed. Exit..");
                    exit(-1);;
                }
            }
            else{
                consumerSmhDetails.read_write_index++;
            }
        }

        if (writeCursor >= sizeof(exhangeResponse)) {
            std::cerr << "Buffer overflow, it should be drained. Exit..." << std::endl;
            exit(-1);; 
        }

        //again a reconnect loop would be needed here instead of hard exits
        const auto received = recv(
            sock, 
            exhangeResponse + writeCursor, 
            sizeof(exhangeResponse) - writeCursor, 
            MSG_DONTWAIT);//no waiting.
        
        if(received == 0){
            std::cerr << "Exchange closed connection. Exit.." << std::endl;
            exit(-1);
        }else        
        if (received > 0) {
            //append new bytes to whatever partial data we might already have
            writeCursor += received; 

            //loop because in case batched multiple 24-byte ExecutionReports into a single recv() call.
            while (writeCursor >= executionReportSize){

                //guaranteed to have at least one full 24-byte struct here.
                auto* report = reinterpret_cast<ExecutionReport*>(exhangeResponse);
                processExecution(report, &producerSmhDetails);
                
                // If there is a fragmented packet, we might have processed 24 bytes but have 10 bytes of the 
                // next report remaining. Put those 10 bytes to the front of the buffer and update the 
                //cursor to wait for the rest.
                size_t remaining = writeCursor - executionReportSize;
                if (remaining > 0) {
                    std::memmove(exhangeResponse, exhangeResponse + executionReportSize, remaining);
                }
                writeCursor = remaining;
            }

        }else{
            if (errno != EAGAIN && errno != EWOULDBLOCK) {
                perror("recv failed. Exit");
                exit(-1);
            }
        }
    
        // Only pause if we did absolutely nothing this iteration
        if (consumerSmhDetails.read_write_index == current_head && received <= 0) {
            __builtin_ia32_pause(); 
        }
    }

    munmap(consumerSmhDetails.pshmMemMmap, consumerSmhDetails.shmSize);
    close(consumerSmhDetails.fd);

    munmap(producerSmhDetails.pshmMemMmap, producerSmhDetails.shmSize);
    close(producerSmhDetails.fd);

    return 0;
}