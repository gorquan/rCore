#include "../../include/module.h"

const char REPEATER[]="The essence of human beings is repeater.\n";
const int REPEATER_LENGTH=sizeof(REPEATER)-1;
struct repeater{
    int offset;
};




uint64_t open(){
    struct repeater* rpt= lkm_api_kmalloc(sizeof(struct repeater));
    rpt->offset=0;
    return rpt;
}
ssize_t read(void* data, char* buf, size_t len){
    struct repeater* rpt=data;
    int i=0;
    while(i<len){
        if(rpt->offset<10*REPEATER_LENGTH){
            buf[i]=REPEATER[rpt->offset%REPEATER_LENGTH];
            i++;
            rpt->offset++;
        }else{
            break;
        }
    }
    return i;
}
ssize_t read_at(void* data, size_t offset, char* buf, size_t len){
    return -1;
}
ssize_t write(void* data, const char* buf, size_t len){
    return -1;
}
ssize_t write_at(void* data, size_t offset, const char* buf, size_t len){
    return -1;
}
ssize_t seek(void* data, uint64_t pos_mode, ssize_t pos){
    return -1;
}
ssize_t set_len(void* data, size_t len){
    return -1;
}
ssize_t sync_all(void* data){
    return -1;
}
ssize_t sync_data(void* data){
    return -1;
}
void* poll(void* data){
    return 0;
}
ssize_t io_control(void* data, uint32_t cmd, uint64_t ctrldata){
    return -1;
}
void close(void* data){
    lkm_api_kfree(data, sizeof(struct repeater));
}

void init_module(){
    struct file_operations ops={
        .open=open,
        .read=read,
        .read_at=read_at,
        .write=write,
        .write_at=write_at,
        .seek=seek,
        .set_len=set_len,
        .sync_all=sync_all,
        .sync_data=sync_data,
        .poll=poll,
        .io_control=io_control,
        .close=close
    };

    struct cdev my_device={
        .parent_module=THIS_MODULE,
        .file_ops=&ops,
        .major=20
    };
    lkm_api_register_device(&my_device);

}
