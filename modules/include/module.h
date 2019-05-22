// C header file for rCore Loadable Kernel Module.
#include "./unistd.h"
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

// Filesystem Subsystem
struct timespec{
    int64_t sec;
    int32_t nsec;
};
struct inode_metadata{
    uint64_t dev;
    uint64_t inode;
    uint64_t size;
    uint64_t blk_size;
    uint64_t blocks;
    struct timespec atime;
    struct timespec mtime;
    struct timespec ctime;
    uint64_t type_;
    uint16_t mode;
    uint64_t nlinks;
    uint64_t uid;
    uint64_t gid;
    uint64_t rdev;
};


struct poll_status{
    uint8_t flags;
};
struct inode_operations{
    ssize_t (*read_at)(void* inode, size_t offset, void* buf, size_t len);
    ssize_t (*write_at)(void* inode, size_t offset, const void* buf, size_t len);
    ssize_t (*metadata)(void* inode, struct inode_metadata* metadata);
    ssize_t (*set_metadata)(void* inode, const struct inode_metadata* metadata);
    ssize_t (*poll)(void* inode, struct poll_status* status);
    ssize_t (*sync_all)(void* inode);
    ssize_t (*sync_data)(void* inode);
    ssize_t (*resize)(void* inode, size_t len);
    ssize_t (*create)(void* inode, const char* name, size_t len, uint64_t type_,
                   uint32_t mode, void** result);
    ssize_t (*setrdev)(void* inode, uint64_t dev);
    ssize_t (*unlink)(void* inode, const char* name, size_t len);
    ssize_t (*link)(void* inode, const char* name, size_t len, void* other);
    ssize_t (*move_)(void* inode, const char* old_name, size_t old_len,
                  void* target, const char* new_name, size_t new_len);
    ssize_t (*find)(void* inode, const char* name, size_t len,
                 void** /* Arc */ result);
    ssize_t (*get_entry)(void* inode, size_t id, void* buffer);
    ssize_t (*io_control)(void* inode, uint32_t cmd, void* data);
    void (*drop)(void* inode);
};
struct fsinfo{
    size_t bsize;
    size_t frsize;
    size_t blocks;
    size_t bfree;
    size_t bavail;
    size_t files;
    size_t ffree;
    size_t namemax;
};
struct filesystem_operations{
    ssize_t (*mount)(uint64_t flags, const char* dev_name, void* data,
                     void* arc_fs, void** result);
    ssize_t (*sync)(void* fs);
    ssize_t (*root_inode)(void* fs, void** /* Arc */ inode);
    ssize_t (*info)(void* fs, struct fsinfo* info);
    void (*drop)(void* fs);
};

ssize_t lkm_api_register_fs(const char* name,
                            const struct filesystem_operations* fsops,
                            const struct inode_operations* inodeops,
                            void* fsdata);
ssize_t lkm_api_create_arc_inode(void* /* Arc */ fs, void* inode);
void lkm_api_release_arc_inode(void* /* Arc */ inode);
// The result does not matter. The reference-counting does.
ssize_t lkm_api_clone_arc_inode(void* /* Arc */ inode);
void lkm_api_info(const char* text);
