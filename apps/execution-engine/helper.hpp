#include "contracts.hpp"
#include <iostream>
#include <filesystem>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <netinet/tcp.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <thread>


constexpr size_t MAX_ORDER_TRACK = 4096;
uint64_t sendTimeStamps[MAX_ORDER_TRACK] = {};
struct ShmDetails {
    void* pshmMemMmap;
    uintmax_t shmSize;
    RingHeader* header;
    int fd;
    uint64_t read_write_index{0};
};

// dump these here...hacky code...should have a class, RAII for fd,shm etc etc
// also make a C++ header only shm library...like the Rust one..only thought about the writer part after
// Refactor later

ShmDetails setupProducerSharedMemory(){
    const std::filesystem::path shmPath = "/dev/shm/hft_exchange_info";
    ShmDetails smhDetails;
    smhDetails.shmSize = 64 * 1024 * 1024;

    smhDetails.fd = open(shmPath.c_str(), O_CREAT| O_RDWR, 0666);

    if(smhDetails.fd < 0){
        perror("hft_exchange_info open failed.");
        exit(-1);
    }

    if (ftruncate(smhDetails.fd, smhDetails.shmSize) < 0) {
        perror("hft_exchange_info ftruncate failed");
        exit(-1);
    }

    void* pshmMemMmap = mmap(
        nullptr,
        smhDetails.shmSize,
        PROT_READ | PROT_WRITE,
        MAP_SHARED,
        smhDetails.fd, 
        0);

    if(pshmMemMmap == MAP_FAILED){
        perror("hft_exchange_info mmap failed");
        close(smhDetails.fd);
        exit(-1);
    }

    auto *header = static_cast<RingHeader*>(pshmMemMmap);

    header->magic_num.store(0x1122334455667788, std::memory_order_release);
    header->head.store(0,std::memory_order_release);

    smhDetails.pshmMemMmap = pshmMemMmap;
    smhDetails.header = header;

    return smhDetails;
}

ShmDetails setupConsumerSharedMemory(){
    const std::filesystem::path shmPath = "/dev/shm/hft_execution";

    std::error_code ec;
    bool found = false;

    for (int i = 0; i < 50; ++i) {
        if (std::filesystem::exists(shmPath, ec)) {
            found = true;
            break;
        }

        if (ec) {
            std::cerr << "System error checking path: " << ec.message() << std::endl;
            exit(-1);
        }

        std::cout << "Waiting for SHM path... (Attempt " << i + 1 << "/50)" << std::endl;
        std::this_thread::sleep_for(std::chrono::seconds(1));
    }

    if (!found) {
        std::cerr << "Rust has not created shm file 50 seconds." << std::endl;
        exit(-1);
    }


    ShmDetails smhDetails;

    smhDetails.shmSize = std::filesystem::file_size(shmPath, ec);
    if (ec) {
        std::cerr << "Cant get file size: " << ec.message() << std::endl;
        exit(-1);
    }

    smhDetails.fd = open(shmPath.c_str(), O_RDWR);

    if(smhDetails.fd < 0){
        perror("Open failed");
        exit(-1);
    }

    void* pshmMemMmap = mmap(
        nullptr,
        smhDetails.shmSize,
        PROT_READ | PROT_WRITE,
        MAP_SHARED,
        smhDetails.fd, 
        0);

    if(pshmMemMmap == MAP_FAILED){
        perror("hft_execution mmap failed");
        close(smhDetails.fd);
        exit(-1);
    }

    smhDetails.pshmMemMmap = pshmMemMmap;
    smhDetails.header = static_cast<RingHeader*>(pshmMemMmap);

    if (smhDetails.header->magic_num.load() == 0x1122334455667788) {
        std::cout << "Successfully attached to rust execution order shared memory." << std::endl;
    }
    else{
        std::cerr << "Unexpected format for rust execution order shared memory." << std::endl;
        exit(-1);
    }

    return smhDetails;
};


int setUpNetwork(){
    auto sock = socket(
        AF_INET,
        SOCK_STREAM, 
        0);

    if(sock < 0){
        perror("socket() failed.");
        exit(-1);
    }

    auto opt = 1;

    if(setsockopt(sock,
        IPPROTO_TCP,
        TCP_NODELAY,
        &opt,
        sizeof(opt))< 0){
            perror("TCP_NODELAY failed.");
            exit(-1);
        }

    int quickack = 1;
    if (setsockopt(
        sock,
        IPPROTO_TCP,
        TCP_QUICKACK,
        &quickack, 
        sizeof(quickack)) < 0) {
            perror("TCP_QUICKACK failed...non-critical)");
        }

    int busy_poll = 50; // microseconds to busy-poll..keep the data in the ring buffer, back for it in nanoseconds
    if (setsockopt(
        sock,
        SOL_SOCKET,
        SO_BUSY_POLL, 
        &busy_poll, 
        sizeof(busy_poll)) < 0){
        perror("SO_BUSY_POLL failed...non-critical)");
    }

    // minimise kernel buffering multiple execution reports before recv call sees them. 64k is fine here..
    int sndbuf = 64 * 1024; 
    int rcvbuf = 64 * 1024;

    if (setsockopt(
        sock,
        SOL_SOCKET,
        SO_SNDBUF,
        &sndbuf,
        sizeof(sndbuf)) < 0)  {
            perror("SO_SNDBUF failed.");
            exit(-1);
        }

    if (setsockopt(
        sock,
        SOL_SOCKET,
        SO_RCVBUF,
        &rcvbuf,
        sizeof(rcvbuf))< 0)  {
            perror("SO_RCVBUF failed.");
            exit(-1);
    }

    struct sockaddr_in serv_addr;
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_port   = htons(9001);

    if(1 != inet_pton(
        AF_INET,
        "127.0.0.1",
        &serv_addr.sin_addr)){
            perror("inet_pton failed.");
            exit(-1);
    }

    if(connect(
        sock, 
        reinterpret_cast<sockaddr*>(&serv_addr), 
        sizeof(serv_addr)) < 0){
        perror("Connection to test exchange failed, make sure python test server is running.");
        exit(-1);;
    }

    return sock;
}