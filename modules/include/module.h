// C header file for rCore Loadable Kernel Module.
typedef unsigned long long uint64_t;
typedef uint64_t size_t;
typedef long long ssize_t;
typedef unsigned int uint32_t;
typedef struct {
    // stub. Only needed to passed to API.
} Module;

extern Module* THIS_MODULE; // Pointer referring to this module.

// API list.

// Hello-world APIs.
uint64_t lkm_api_pong();
uint64_t lkm_api_debug();

// Symbol-related APIs.
uint64_t lkm_api_query_symbol(const char* symbol);

// The entrance of all modules.
void init_module();

// CDev Subsystem

struct file_operations{
    uint64_t (*open)();
    ssize_t (*read)(void* data, char* buf, size_t len);
    ssize_t (*read_at)(void* data, size_t offset, char* buf, size_t len);
    ssize_t (*write)(void* data, const char* buf, size_t len);
    ssize_t (*write_at)(void* data, size_t offset, const char* buf, size_t len);
    ssize_t (*seek)(void* data, uint64_t pos_mode, ssize_t pos);
    ssize_t (*set_len)(void* data, size_t len);
    ssize_t (*sync_all)(void* data);
    ssize_t (*sync_data)(void* data);
    void* (*poll)(void* data);
    ssize_t (*io_control)(void* data, uint32_t cmd, uint64_t ctrldata);
    void (*close)(void* data);
};

struct cdev{
    Module* parent_module;
    struct file_operations* file_ops;
    uint32_t major;
};

ssize_t lkm_api_register_device(struct cdev* dev);
void* lkm_api_kmalloc(size_t size);
void lkm_api_kfree(void* ptr, size_t size);
