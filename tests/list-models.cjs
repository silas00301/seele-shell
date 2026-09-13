const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {nativeBridge,source}=require('./native-functions.cjs');
module.exports=function(){const context=vm.createContext({Bridge:nativeBridge()});vm.runInContext(source(fs.readFileSync(process.env.SEELE_QML_LIST_MODELS || path.resolve(__dirname,'../projects/shared/ListModels.js'),'utf8')),context);return context;};
