void pong_twice();
typedef struct {
} Module;
extern Module THIS_MODULE;
void lkm_api_debug(Module* this_module);
typedef unsigned long long uint64_t;
uint64_t lkm_api_query_symbol(const char* symbol);

void init_module(){
    lkm_api_debug(&THIS_MODULE);
    pong_twice();
    lkm_api_debug(&THIS_MODULE);
    void (*dyn_pong_twice)()=lkm_api_query_symbol("pong_twice");
    if(dyn_pong_twice){
        (*dyn_pong_twice)();
    }
}

void cleanup_module(){
    pong_twice();
}
