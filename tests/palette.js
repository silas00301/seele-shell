const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[2],'utf8').replace(/^\.pragma library\s*/,''),context);
// Golden fallback values from main@origin; fixture data intentionally records
// the visual contract independently of the implementation's shared map.
const golden = {
 base:'#1e1e2e', mantle:'#181825', crust:'#11111b', surface:'#313244',
 overlay:'#6c7086', text:'#cdd6f4', subtext:'#a6adc8', accent:'#b4befe',
 red:'#f38ba8', green:'#a6e3a1', yellow:'#f9e2af', fontFamily:'Maple Mono NF CN',
 wallpaper:'/etc/wallpaper/wallpaper.jpg'
};
assert.deepEqual(JSON.parse(JSON.stringify(context.fallback)),golden);
assert.equal(Object.isFrozen(context.fallback),true);
for (const source of process.argv.slice(3)) {
 const qml=fs.readFileSync(source,'utf8');
 const properties=Array.from(qml.matchAll(/property (?:color|string) (\w+): Palette\.fallback\.(\w+)/g));
 assert(properties.length>=11,'each surface consumes shared defaults');
 for (const match of properties) assert.equal(match[1],match[2]);
 assert.match(qml,/Palette\.assign\(root, theme(?:, true)?\)/);
 assert.doesNotMatch(qml,/root\.base = theme\.base/);
 assert.doesNotMatch(qml,/property color (?:base|mantle|crust|accent): "#/);
}
let target={...golden,other:'unchanged'};
context.assign(target,{base:'#112233',fontFamily:'Fixture',wallpaper:'/new'},false);
assert.equal(target.base,'#112233');assert.equal(target.fontFamily,'Fixture');
assert.equal(target.wallpaper,golden.wallpaper);
context.assign(target,{base:'',fontFamily:null,accent:false,other:'changed',wallpaper:'/new'},true);
assert.equal(target.base,'#112233');assert.equal(target.fontFamily,'Fixture');
assert.equal(target.accent,golden.accent);assert.equal(target.other,'unchanged');assert.equal(target.wallpaper,'/new');
context.assign(target,Object.create({base:'#ffffff'}));assert.equal(target.base,'#112233');
const subset={base:golden.base,fontFamily:golden.fontFamily};
context.assign(subset,{green:'#ffffff',base:'#123456'});
assert.deepEqual(subset,{base:'#123456',fontFamily:golden.fontFamily});
assert.throws(()=>context.assign(target,null));
console.log('Shared palette golden defaults, consumer bindings, partial/falsy overrides and assignment boundaries passed');
