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
// Material roles: the golden dark palette's derivation is recorded, and a
// light palette reverses the surface ramp and the inks without a mode flag.
const dark=context.roles(golden);
assert.equal(dark.darkScheme,true);
assert.equal(dark.primary,'#ffb4befe');
assert.equal(dark.textOnPrimary,'#ff11111b');
assert.equal(dark.primaryContainer,'#ff4b4e6c');
assert.equal(dark.secondaryContainer,'#ff484a63');
assert.equal(dark.surfaceContainerLowest,'#ff11111b');
assert.equal(dark.surfaceContainerHighest,'#ff333446');
const latte={...golden,base:'#eff1f5',mantle:'#e6e9ef',crust:'#dce0e8',surface:'#ccd0da',overlay:'#9ca0b0',
 text:'#4c4f69',subtext:'#6c6f85',accent:'#7287fd',red:'#d20f39',green:'#40a02b',yellow:'#df8e1d'};
const light=context.roles(latte);
assert.equal(light.darkScheme,false);
assert.equal(light.textOnPrimary,'#ffeff1f5');
assert.equal(light.textOnPrimaryContainer.length,9);
const lum=c=>context.luminance(c);
for (const scheme of [dark,light]) {
 const ramp=['surfaceContainerLowest','surfaceContainerLow','surfaceContainer','surfaceContainerHigh','surfaceContainerHighest'].map(k=>lum(scheme[k]));
 const step=scheme.darkScheme?1:-1;
 for (let i=1;i<ramp.length;i++) assert(step*(ramp[i]-ramp[i-1])>0,'the surface ramp moves towards the text');
 for (const [fill,ink] of [['primary','textOnPrimary'],['primaryContainer','textOnPrimaryContainer'],['secondaryContainer','textOnSecondaryContainer'],['errorContainer','textOnErrorContainer']]) {
  const [a,b]=[lum(scheme[fill]),lum(scheme[ink])].sort((x,y)=>y-x);
  assert((a+0.05)/(b+0.05)>=2.5,fill+' carries legible content');
 }
}
assert.equal(context.inkOn('#ffffff',golden),'#ff11111b');
assert.equal(context.inkOn('#000000',golden),'#ffcdd6f4');
assert.equal(context.tone('#000000','#ffffff',0.5),'#ff808080');
assert.throws(()=>context.channels('#12'));
console.log('Shared palette golden defaults, consumer bindings, partial/falsy overrides, assignment boundaries and Material roles passed');
