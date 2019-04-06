use alloc::vec::*;
use alloc::string::*;
use super::kernelvm::*;
pub struct ModuleSymbol{
    pub name: String,
    pub loc: usize

}
pub struct ModuleDependence{
    pub name: String,
    pub api_version: i32
}
pub struct ModuleInfo{
    pub name: String,
    pub version: i32,
    pub api_version: i32,
    pub exported_symbols: Vec<String>,
    pub dependent_modules: Vec<ModuleDependence>

}

impl ModuleInfo{
    pub fn parse(input:&str)->Option<ModuleInfo>{
        let lines: Vec<&str>=input.split('\n').collect();
        let mut minfo=ModuleInfo{
            name:String::from("<anonymous module>"),
            version:0,
            api_version:0,
            exported_symbols:Vec::new(),
            dependent_modules:Vec::new()};
        for l in lines{
            if l.len()==0 {
                continue;
            }
            let columns: Vec<&str>=l.split(':').collect();
            if columns.len()!=2{
                return None;
            }
            match columns[0]{
                "name" => {
                    minfo.name=String::from(columns[1]);
                }
                "version"=>{
                    minfo.version=columns[1].parse::<i32>().unwrap();
                }
                "api_version"=>{
                    minfo.api_version=columns[1].parse::<i32>().unwrap();
                }
                "exported_symbols"=>{
                    let symbols : Vec<&str>=columns[1].split(",").collect();
                    minfo.exported_symbols=symbols.iter().map(|s| String::from(*s)).collect();
                }
                "dependence"=>{}
                _ => {return None;}

            }
        }
        Some(minfo)
    }

}



pub struct LoadedModule<'a>{
    pub info: ModuleInfo,
    pub exported_symbols: Vec<ModuleSymbol>,
    pub used_counts: i32,
    pub vspace: VirtualSpace<'a>
}

