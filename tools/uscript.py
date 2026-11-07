r"""Turns UnrealScript functions in a Fall of Cybertron package back into readable
pseudo-code (a rough decompiler for the game's UE3 bytecode). Used to read how the
game's scripts move and animate things, e.g. the car (TnCarForm, TnHoverCarSimulation
in TransGame.xxx).

usage: python uscript.py <package .xxx> <Class.Function | Class.State.Function> ...

Operators and other native functions are named from Core.xxx, Engine.xxx and
HM_Engine.xxx when they are next to the package (in TransGame\CookedPC).
Statements print with their offset in the stored bytecode; jump targets ("goto @...")
are offsets in the game's in-memory bytecode, where object references take 8 bytes,
so they run a little past the printed offsets.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "ue3"))
from ue3pkg import Package  # noqa: E402

NAT = {}


def load_natives(folder, main):
    """Native function numbers -> names, from the packages that declare them."""
    for f in ("Core.xxx", "Engine.xxx", "HM_Engine.xxx", os.path.basename(main)):
        path = os.path.join(folder, f)
        if not os.path.exists(path):
            continue
        p = main_pkg if os.path.abspath(path) == os.path.abspath(main) else Package(path)
        for i, e in enumerate(p.exports):
            if p.class_name(e) != "Function":
                continue
            d = p.data[e["offset"]:e["offset"] + e["size"]]
            if len(d) < 60:
                continue
            nat, _op, flags = struct.unpack_from("<HBI", d, len(d) - 11)
            if nat and flags & 0x400:
                NAT.setdefault(nat, p.full_name(i + 1).split(".")[-1])


class D:
    def __init__(self, p, d, o, end):
        self.p, self.d, self.o, self.end = p, d, o, end
        # Object references take 8 bytes in memory (4 on disk): the jump offsets
        # are memory offsets, so each statement's is its disk offset plus 4 per
        # reference before it.
        self.extra = 0
    def u8(self): v=self.d[self.o]; self.o+=1; return v
    def u16(self): v=struct.unpack_from('<H',self.d,self.o)[0]; self.o+=2; return v
    def i32(self): v=struct.unpack_from('<i',self.d,self.o)[0]; self.o+=4; return v
    def f32(self): v=struct.unpack_from('<f',self.d,self.o)[0]; self.o+=4; return v
    def obj(self):
        i=self.i32(); self.extra+=4
        if i==0: return 'None'
        n=self.p.full_name(i)
        return n.split('.')[-1]
    def objfull(self):
        i=self.i32(); self.extra+=4; return self.p.full_name(i) if i else 'None'
    def name(self):
        i=self.i32(); n=self.i32()
        s=self.p.names[i] if 0<=i<len(self.p.names) else '?%d'%i
        return s if n==0 else '%s_%d'%(s,n-1)
    def parms(self):
        a=[]
        while self.d[self.o]!=0x16:
            a.append(self.expr())
        self.o+=1
        return a
    def expr(self):
        start=self.o
        t=self.u8()
        if t==0x00: return self.obj()
        if t==0x01: return self.obj()
        if t==0x02: return 'default.'+self.obj()
        if t==0x04: return 'return '+self.expr()
        if t==0x05: sz=self.u16(); return 'switch(%s)'%self.expr()
        if t==0x06: return 'goto @%04x'%self.u16()
        if t==0x07: off=self.u16(); return 'if !(%s) goto @%04x'%(self.expr(),off)
        if t==0x08: return 'stop'
        if t==0x09: ln=self.u16(); self.u8(); return 'assert(%s)'%self.expr()
        if t==0x0A:
            off=self.u16()
            if off==0xFFFF: return 'default:'
            return 'case %s: (next @%04x)'%(self.expr(),off)
        if t==0x0B: return 'nothing'
        if t==0x0D: return 'goto label '+self.expr()
        if t==0x0E: return self.obj() and self.expr()
        if t==0x0F: a=self.expr(); b=self.expr(); return '%s = %s'%(a,b)
        if t==0x10: i=self.expr(); a=self.expr(); return '%s[%s]'%(a,i)
        if t==0x12 or t==0x19:
            a=self.expr(); skip=self.u16(); sz=self.u8(); ty=self.u8(); b=self.expr()
            return '%s.%s'%(a,b)
        if t==0x51: return self.expr()
        if t==0x13: c=self.obj(); return 'class<%s>(%s)'%(c,self.expr())
        if t==0x14: a=self.expr(); b=self.expr(); return '%s = %s'%(a,b)
        if t==0x15: return 'EndParmValue'
        if t==0x17: return 'self'
        if t==0x18: self.u16(); return self.expr()
        if t==0x1A: i=self.expr(); a=self.expr(); return '%s[%s]'%(a,i)
        if t==0x1B: n=self.name(); return '%s(%s)'%(n, ', '.join(self.parms()))
        if t==0x1C: n=self.obj(); return '%s(%s)'%(n, ', '.join(self.parms()))
        if t==0x1D: return str(self.i32())
        if t==0x1E: return repr(round(self.f32(),6))
        if t==0x1F:
            e=self.d.index(b'\0',self.o); s=self.d[self.o:e].decode('latin-1'); self.o=e+1; return repr(s)
        if t==0x20: return 'obj:'+self.objfull()
        if t==0x21: return "'%s'"%self.name()
        if t==0x22: return 'rot(%d,%d,%d)'%(self.i32(),self.i32(),self.i32())
        if t==0x23: return 'vect(%g,%g,%g)'%(self.f32(),self.f32(),self.f32())
        if t==0x24: return str(self.u8())
        if t==0x25: return '0'
        if t==0x26: return '1'
        if t==0x27: return 'true'
        if t==0x28: return 'false'
        if t==0x29: return 'parm '+self.obj()
        if t==0x2A: return 'None'
        if t==0x2B: return 'tok2b'  # (FoC: a one-byte token seen only in a class context)
        if t==0x2C: return str(self.u8())
        if t==0x2D: return self.expr()
        if t==0x2E: c=self.obj(); return '%s(%s)'%(c,self.expr())
        if t==0x2F: e=self.expr(); self.u16(); return 'foreach '+e
        if t==0x30: return 'iteratorpop'
        if t==0x31: return 'iteratornext'
        if t in (0x32,0x33): s=self.obj(); a=self.expr(); b=self.expr(); return '(%s %s %s)'%(a,'=='if t==0x32 else '!=',b)
        if t==0x35: f=self.obj(); s=self.obj(); self.u8(); self.u8(); e=self.expr(); return '%s.%s'%(e,f)
        if t==0x36: return '%s.Length'%self.expr()
        if t==0x37: n=self.name(); return 'global.%s(%s)'%(n, ', '.join(self.parms()))
        if t==0x38: ty=self.u8(); return 'cast%02x(%s)'%(ty,self.expr())
        if t==0x3A: self.obj(); return 'return'
        if t==0x40: a=self.expr(); b=self.expr(); c=self.expr(); return '%s.Remove(%s,%s)'%(a,b,c)
        if t==0x39: a=self.expr(); b=self.expr(); c=self.expr(); return '%s.Insert(%s,%s)'%(a,b,c)
        if t==0x41: self.i32(); self.i32(); self.i32(); self.u8(); return None
        if t==0x42: self.u8(); o=self.obj(); n=self.name(); return '%s(%s)'%(n, ', '.join(self.parms()))
        if t==0x43: n=self.name(); return 'delegate '+n  # (FoC: the function's name only)
        if t==0x44: a=self.expr(); b=self.expr(); return '%s = %s'%(a,b)
        if t==0x45: c=self.expr(); self.u16(); a=self.expr(); self.u16(); b=self.expr(); return '(%s ? %s : %s)'%(c,a,b)
        if t==0x46: a=self.expr(); self.u16(); b=self.expr(); self.u8() if False else None; return '%s.Find(%s)'%(a,b)
        if t==0x47: a=self.expr(); self.u16(); m=self.expr(); b=self.expr(); return '%s.Find(%s, %s)'%(a,m,b)
        if t==0x48: return self.obj()
        if t==0x49: self.u16(); e=self.expr(); self.u8(); return 'defaultparm %s'%e
        if t==0x4A: return 'emptyparm'
        if t==0x4B: return self.name()
        if t==0x52: c=self.obj(); return '%s(%s)'%(c,self.expr())
        if t==0x53: return 'EOS'
        if t==0x58:
            a=self.expr(); it=self.expr(); has=self.u8(); ix=self.expr(); self.u16()
            ix=ix if has else None
            return 'foreach %s(%s%s)'%(a,it,', '+ix if ix else '')
        if t==0x11:
            # new(Outer, Name) Class(Template): FoC's (High Moon's) form has its four
            # expressions, then the class's constructor function (an object
            # reference, 0 for none) and the constructor's arguments up to
            # EX_EndFunctionParms.
            a=[self.expr() for _ in range(4)]
            ctor=self.i32(); self.extra+=4
            args=self.parms()
            s='new(%s)'%', '.join(a)
            if ctor:
                s+=' %s(%s)'%(self.p.full_name(ctor).split('.')[-1], ', '.join(args))
            return s
        if t in (0x3B, 0x3C, 0x3D, 0x3E):
            # Delegate comparisons, written like calls: their operands up to
            # EX_EndFunctionParms.
            op = {0x3B: '==', 0x3C: '!=', 0x3D: '==', 0x3E: '!='}[t]
            return '(%s)' % (' %s ' % op).join(self.parms())
        if t==0x3F: return 'None'
        if t==0x56: a=self.expr(); self.u16(); b=self.expr(); return '%s.RemoveItem(%s)'%(a,b)
        if t==0x57: a=self.expr(); self.u16(); b=self.expr(); c=self.expr(); return '%s.InsertItem(%s, %s)'%(a,b,c)
        if t==0x54: a=self.expr(); self.u16(); b=self.expr(); return '%s.Add(%s)'%(a,b)
        if t==0x55: a=self.expr(); self.u16(); b=self.expr(); return '%s.AddItem(%s)'%(a,b)
        if t==0x16: return ')'
        if 0x60<=t<0x70:
            idx=(t-0x60)*256+self.u8()
            n=NAT.get(idx,'native%d'%idx); return '%s(%s)'%(n, ', '.join(self.parms()))
        if t>=0x70:
            n=NAT.get(t,'native%d'%t); return '%s(%s)'%(n, ', '.join(self.parms()))
        raise ValueError('token %02x at %x'%(t,start))

def decompile(p, fullname):
    idx={p.full_name(i+1):i for i in range(len(p.exports))}
    e=p.exports[idx[fullname]]
    d=p.data[e['offset']:e['offset']+e['size']]
    o=48
    x=D(p,d,o,len(d)-11)
    lines=[]
    while x.o < len(d)-11:
        at=x.o-48+x.extra
        try:
            s=x.expr()
        except Exception as ex:
            lines.append('  !! %s'%ex); break
        if s is None: continue
        if s=='EOS': break
        lines.append('  @%04x %s'%(at,s))
    return lines

if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main_pkg = Package(sys.argv[1])
    load_natives(os.path.dirname(os.path.abspath(sys.argv[1])), sys.argv[1])
    for fn in sys.argv[2:]:
        print(fn)
        print("\n".join(decompile(main_pkg, fn)))
