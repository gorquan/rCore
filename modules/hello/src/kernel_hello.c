unsigned long long lkm_api_pong();
void pong_twice();
char buffer[2000];
void init_module(){
    int i=0;
    for(i=0;i<2000;i++){
        buffer[i]=i;
    }
    lkm_api_pong();
    pong_twice();
}
