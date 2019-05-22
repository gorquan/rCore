#include "../../include/module.h"
#include "./printf.h"
// It may sound funny that both arc_ramfs and arc_inode are weak pointers.
// This is because they are indeed moved out, instead of holding a clone.
// You may see them as &Arc<T> (with proper lifetime automatically), which is easier to understand.
struct ramfs{
    void* arc_ramfs;
    struct fsinfo info;
    struct ramfs_inode* root;
};
struct ramfs_dirent{
    char name[256];
    struct ramfs_inode* item; // Points to arc.
};
struct ramfs_inode{
    struct inode_metadata metadata;
// There are two situations for an inode.
// 1. This is the root inode. In this case the inode works as a "static" one and requires manual recycling when filesystem is released.
// 2. This is an inode attached to some other inode. In this case, it is created with two links (one for returning and one for linking), and works as proper as regular Arcs. So using Arc for linking and unlinking gets the right result.
// In both cases, arc_inode can be seen "attached onto" an existed Arc, so it has same lifecycle as the inode itself(which is trivial, since self-reference always lives as long as the object).
    struct ramfs* parent;    
    void* arc_inode;
    // And overlaying variables
    size_t buf_len;
    char* data;
    size_t links; //This does not matter when recycling.
};
int strcmp(const char* a, const char* b){
    while(1){
        if(*a==*b && *b=='\0') return 0;
        if(*a==*b){
            a++;
            b++;
        }else{
            return 1;
        }
    }
}
size_t strncpy(char* dst, const char* src, size_t n){
    off_t i;
    for(i=0;i<n;i++){
        dst[i]=src[i];
        if(src[i]=='\0') break;
    }
    return i;
}
ssize_t ramfs_resize(void* inode, size_t len){
    char buffer[1024];
    snprintf(buffer, 1024, "ramfs_resize: %ld %ld", inode, len);
    lkm_api_info(buffer);
    struct ramfs_inode* fi=inode;
    if(len==fi->buf_len) return 0;
    if(len==0){
        if(fi->data!=0) lkm_api_kfree(fi->data, fi->buf_len);
        fi->data=0;
        fi->buf_len=0;
        return 0;
    }
    char* new_buffer=lkm_api_kmalloc(len);
    if(new_buffer==0){
        return -1;
    }
    if(fi->buf_len>len){
        //shrinking
        off_t i;
        for(i=0;i<len;i++){
            new_buffer[i]=fi->data[i];
        }
    }else{
        //expanding
        off_t i;
        for(i=0;i<fi->buf_len;i++){
            new_buffer[i]=fi->data[i];        
        }
        for(;i<len;i++){
            new_buffer[i]=0; //Zeroing
        }
    }
    if(fi->data!=0) lkm_api_kfree(fi->data, fi->buf_len);
    fi->data=new_buffer;
    fi->buf_len=len;
    return 0;
}
ssize_t ramfs_find(void* inode, const char* name, size_t len,
 void** /* Arc */ result){
    struct ramfs_inode* fi=inode;
    if(fi->metadata.type_!=1) return -4;
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    off_t i;
    for(i=0;i<total_files;i++){
	lkm_api_info(entries[i].name);
        if(strcmp(entries[i].name, name)==0){
            //found.
            *result=lkm_api_clone_arc_inode(entries[i].item->arc_inode);
            return 0;
        }
    }
    return -5;
}
struct ramfs_inode* alloc_inode(struct ramfs* fs){
    struct ramfs_inode* inode=lkm_api_kmalloc(sizeof(struct ramfs_inode));
    off_t i;
    for(i=0;i<sizeof(struct ramfs_inode);i++){
        ((char*)inode)[i]=0;
    }
    lkm_api_info("create_arc_inode");
    inode->arc_inode=lkm_api_create_arc_inode(fs->arc_ramfs, inode);
    inode->parent=fs;
    return inode;
}
void free_inode(struct ramfs_inode* inode){
    ramfs_resize(inode, 0);
    lkm_api_kfree(inode, sizeof(*inode));
}


ssize_t ramfs_read_at(void* inode, size_t offset, void* buf, size_t len){
    struct ramfs_inode* fi=inode;
    size_t remaining=fi->buf_len-offset;
    size_t read_len=(remaining<len?remaining:len);
    off_t off;
    char* cbuf=buf;
    for(off=0;off<read_len;off++){
        cbuf[off]=fi->data[offset+off];
    }
    return read_len;
}
ssize_t ramfs_write_at(void* inode, size_t offset, const void* buf, size_t len){
    char buffer[1024];
    snprintf(buffer, 1024, "ramfs_write_at: %ld %ld %ld %ld", inode, offset, buf, len);
    lkm_api_info(buffer);
    struct ramfs_inode* fi=inode;
    size_t max_size=offset+len;
    if(max_size>fi->buf_len){
        ramfs_resize(inode, max_size);
    }
    lkm_api_info("copy");
    off_t off=0;
    const char* cbuf=buf;
    for(off=0;off<len;off++){
        fi->data[offset+off]=cbuf[off];
    }
    lkm_api_info("copy done");
    return len;
}
ssize_t ramfs_metadata(void* inode, struct inode_metadata* metadata){

    struct ramfs_inode* fi=inode;
    char buffer[1024];
    snprintf(buffer, 1024, "ramfs_metadata: %ld %ld %ld", inode, metadata, fi->metadata.type_);
    lkm_api_info(buffer);
    *metadata=fi->metadata;
    metadata->size=fi->buf_len;
    metadata->blk_size=4096;
    metadata->blocks=((fi->buf_len+4095)/4096);
    metadata->nlinks=fi->links;
    metadata->inode=inode;
    return 0;
}
ssize_t ramfs_set_metadata(void* inode, const struct inode_metadata* metadata){
    struct ramfs_inode* fi=inode;
    fi->metadata=*metadata;
}
ssize_t ramfs_poll(void* inode, struct poll_status* status){
    struct ramfs_inode* fi=inode;
    status->flags=3;
    return 0;
}
ssize_t ramfs_sync_all(void* inode){
    struct ramfs_inode* fi=inode;
    return 0;
}
ssize_t ramfs_sync_data(void* inode){
    struct ramfs_inode* fi=inode;
    return 0;
}


void folder_append_item(struct ramfs_inode* fi, struct ramfs_dirent* dirent){
    ramfs_write_at(fi, fi->buf_len, dirent, sizeof(*dirent));
}
void folder_remove_item(struct ramfs_inode* fi, off_t index){
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    entries[index]=entries[total_files-1];
    ramfs_resize(fi, fi->buf_len-sizeof(struct ramfs_dirent));
}
void init_folder(struct ramfs_inode* fi, struct ramfs_inode* parent){
    fi->metadata.type_=1; // Set metadata as directory.
    struct ramfs_dirent init_dirent;
    init_dirent.name[0]='.';
    init_dirent.name[1]='\0';
    init_dirent.item=fi; // This does not count as a reference.
    folder_append_item(fi, &init_dirent);
    init_dirent.name[1]='.';
    init_dirent.name[2]='\0';
    init_dirent.item=parent; // This does not count as a reference, either.
    // A folder can only be removed empty, and can only be linked once.
    // So removing the folder without hesitate about reference counting is safe.
    folder_append_item(fi, &init_dirent);
}

ssize_t ramfs_create(void* inode, const char* name, size_t len, uint64_t type_,
   uint32_t mode, void** result){
    struct ramfs_inode* fi=inode;
    if(fi->metadata.type_!=1) return -4;
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    off_t i;
    for(i=0;i<total_files;i++){
        if(strcmp(entries[i].name, name)==0){
            return -6;
        }
    }
    struct ramfs_inode* new_inode=alloc_inode(fi->parent);
    new_inode->metadata.type_=type_;
    if(type_==1){
        init_folder(new_inode, fi);
    }
    struct ramfs_dirent tmpent;
    strncpy(tmpent.name, name, 256);
    tmpent.item=new_inode;
    folder_append_item(fi, &tmpent);
    new_inode->links=1;
    *result=lkm_api_clone_arc_inode(new_inode->arc_inode);
    return 0;
}
ssize_t ramfs_setrdev(void* inode, uint64_t dev){
    struct ramfs_inode* fi=inode;
    fi->metadata.rdev=dev;
    return 0;
}
ssize_t ramfs_unlink(void* inode, const char* name, size_t len){
    if(strcmp(name, ".")==0) return -3;
    if(strcmp(name, "..")==0) return -3;
    struct ramfs_inode* fi=inode;
    if(fi->metadata.type_!=1) return -4;
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    off_t i;
    for(i=0;i<total_files;i++){
        if(strcmp(entries[i].name, name)==0){
            //found.
            struct ramfs_inode* target=entries[i].item;
            if(target->metadata.type_==1){
                if(target->buf_len>sizeof(struct ramfs_dirent)*2){
                    return -11; // Directory not empty!
                }
                lkm_api_release_arc_inode(target->arc_inode);
                folder_remove_item(fi, i);
                return 0;
            }else{
                target->links--;
                lkm_api_release_arc_inode(target->arc_inode);
                folder_remove_item(fi, i);
                return 0;
            }
        }
    }
    return -5;
}
ssize_t ramfs_link(void* inode, const char* name, size_t len, void* other){
    struct ramfs_inode* fi=inode;
    struct ramfs_inode* other_fi=other;
    if(other_fi->metadata.type_==1) return -3;
    if(fi->metadata.type_!=1) return -4;
    void* temp;
    if(ramfs_find(inode, name, len, &temp)==0){
        lkm_api_release_arc_inode(temp);
        return -6;
    }
    lkm_api_clone_arc_inode(other_fi->arc_inode); //increase arc.
    // and "bind" it to the folder.
    struct ramfs_dirent tmpent;
    strncpy(tmpent.name, name, 256);
    tmpent.item=other_fi;
    folder_append_item(fi, &tmpent);
    return 0;
}
ssize_t ramfs_move_(void* inode, const char* old_name, size_t old_len,
  void* target, const char* new_name, size_t new_len){
    struct ramfs_inode* fi=inode;
    struct ramfs_inode* other_fi=target;
    if(fi->metadata.type_!=1) return -4;
    if(other_fi->metadata.type_!=1) return -4;
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    off_t i;
    for(i=0;i<total_files;i++){
        if(strcmp(entries[i].name, old_name)==0){
            //found.
            void* temp;
            if(ramfs_find(other_fi, new_name, new_len, &temp)==0){
                lkm_api_release_arc_inode(temp); //duplicate and can't move
                return -6;
            }
            struct ramfs_dirent tmpent=entries[i];
            // appending self element again may crash while vector-moving.
            // we copy entries[i] out to prevent this.
            folder_append_item(other_fi, &tmpent);
            folder_remove_item(fi, i);
            return 0;
        }
    }
    return -5;
}

ssize_t ramfs_get_entry(void* inode, size_t id, void* buffer){
    struct ramfs_inode* fi=inode;
    if(fi->metadata.type_!=1) return -4;
    struct ramfs_dirent* entries=fi->data;
    size_t total_files=fi->buf_len/sizeof(struct ramfs_dirent);
    if(id>=total_files){
        return -5;
    }
    strncpy(buffer, &entries[id], 256);
    return 0;
}
ssize_t ramfs_io_control(void* inode, uint32_t cmd, void* data){
    struct ramfs_inode* fi=inode;
    return -1;
}
void ramfs_drop(void* inode){
    struct ramfs_inode* fi=inode; 
    // For ramfs: now should do the cleanup.
    free_inode(inode);
}

struct inode_operations ramfs_inode_ops={
    .read_at=ramfs_read_at,
    .write_at=ramfs_write_at,
    .metadata=ramfs_metadata,
    .poll=ramfs_poll,
    .sync_all=ramfs_sync_all,
    .sync_data=ramfs_sync_data,
    .resize=ramfs_resize,
    .create=ramfs_create,
    .setrdev=ramfs_setrdev,
    .unlink=ramfs_unlink,
    .link=ramfs_link,
    .move_=ramfs_move_,
    .find=ramfs_find,
    .get_entry=ramfs_get_entry,
    .io_control=ramfs_io_control,
    .drop=ramfs_drop
};

ssize_t ramfs_fs_mount(uint64_t flags, const char* dev_name, void* data,
                     void* arc_fs, void** result){
    char buffer[1024];
    snprintf(buffer, 1024, "mounting onto %ld\n", arc_fs);
    lkm_api_info("Start mounting");
    lkm_api_info(buffer);
    struct ramfs* fs=lkm_api_kmalloc(sizeof(struct ramfs*));
    lkm_api_info("malloc");
    off_t i;
    lkm_api_info("memset");
    for(i=0;i<sizeof(*fs);i++) ((char*)fs)[i]=0; //memset
    fs->arc_ramfs=arc_fs;
    lkm_api_info("alloc_inode");
    struct ramfs_inode* root=alloc_inode(fs);
    root->links=1;
    lkm_api_info("init_folder");
    init_folder(root, root);
    lkm_api_info("set");
    fs->root=root;
    *result=fs;
    lkm_api_info("done");
    return 0;
}
ssize_t ramfs_fs_sync(void* fs){
    return 0;
}
ssize_t ramfs_fs_root_inode(void* fs, void** /* Arc */ inode){
    struct ramfs* rfs=fs;
    *inode=lkm_api_clone_arc_inode(rfs->root->arc_inode);
    return 0;
}
ssize_t ramfs_fs_info(void* fs, struct fsinfo* info){
    struct ramfs* rfs=fs;
    return 0;
}
void ramfs_fs_drop(void* fs){
    lkm_api_info("Drop FS!");
    struct ramfs* rfs=fs;
    lkm_api_release_arc_inode(rfs->root->arc_inode);
    lkm_api_kfree(fs, sizeof(struct ramfs));
}
struct filesystem_operations ramfs_filesystem_ops={
    .mount=ramfs_fs_mount,
    .sync=ramfs_fs_sync,
    .root_inode=ramfs_fs_root_inode,
    .info=ramfs_fs_info,
    .drop=ramfs_fs_drop
};
void _putchar(char chr){/*unused*/}
void init_module(){
    char buffer[1024];
    snprintf(buffer, 1024, "external function: %ld %ld", ramfs_inode_ops, ramfs_inode_ops.metadata);
    lkm_api_info(buffer);
    lkm_api_register_fs("ramfs", &ramfs_filesystem_ops, &ramfs_inode_ops, 0);   
}
