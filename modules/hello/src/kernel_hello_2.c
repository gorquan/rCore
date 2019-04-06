unsigned long long lkm_api_pong();
void pong_twice(){
    lkm_api_pong();
    lkm_api_pong();
}
