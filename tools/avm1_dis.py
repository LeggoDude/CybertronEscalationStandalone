"""Tiny AVM1 (ActionScript 2) disassembler, for reading a movie's HUD logic."""
import struct
import sys

OPS = {0x04: "nextFrame", 0x05: "prevFrame", 0x06: "play", 0x07: "stop", 0x0A: "add", 0x0B: "subtract",
       0x0C: "multiply", 0x0D: "divide", 0x0E: "equals", 0x0F: "less", 0x10: "and", 0x11: "or",
       0x12: "not", 0x13: "stringEquals", 0x14: "stringLength", 0x15: "stringExtract", 0x17: "pop",
       0x18: "toInteger", 0x1C: "getVariable", 0x1D: "setVariable", 0x20: "setTarget2",
       0x21: "stringAdd", 0x22: "getProperty", 0x23: "setProperty", 0x24: "cloneSprite",
       0x25: "removeSprite", 0x26: "trace", 0x27: "startDrag", 0x28: "endDrag", 0x29: "stringLess",
       0x2A: "throw", 0x2B: "castOp", 0x2C: "implementsOp", 0x30: "randomNumber", 0x31: "mbStringLength",
       0x32: "charToAscii", 0x33: "asciiToChar", 0x34: "getTime", 0x35: "mbStringExtract",
       0x36: "mbCharToAscii", 0x37: "mbAsciiToChar", 0x3A: "delete", 0x3B: "delete2",
       0x3C: "defineLocal", 0x3D: "callFunction", 0x3E: "return", 0x3F: "modulo", 0x40: "newObject",
       0x41: "defineLocal2", 0x42: "initArray", 0x43: "initObject", 0x44: "typeOf",
       0x45: "targetPath", 0x46: "enumerate", 0x47: "add2", 0x48: "less2", 0x49: "equals2",
       0x4A: "toNumber", 0x4B: "toString", 0x4C: "pushDuplicate", 0x4D: "stackSwap",
       0x4E: "getMember", 0x4F: "setMember", 0x50: "increment", 0x51: "decrement",
       0x52: "callMethod", 0x53: "newMethod", 0x54: "instanceOf", 0x55: "enumerate2",
       0x60: "bitAnd", 0x61: "bitOr", 0x62: "bitXor", 0x63: "bitLShift", 0x64: "bitRShift",
       0x65: "bitURShift", 0x66: "strictEquals", 0x67: "greater", 0x68: "stringGreater",
       0x69: "extends", 0x81: "gotoFrame", 0x83: "getURL", 0x87: "storeRegister",
       0x88: "constantPool", 0x8A: "waitForFrame", 0x8B: "setTarget", 0x8C: "goToLabel",
       0x8D: "waitForFrame2", 0x8E: "defineFunction2", 0x8F: "try", 0x94: "with", 0x96: "push",
       0x99: "jump", 0x9A: "getURL2", 0x9B: "defineFunction", 0x9D: "if", 0x9E: "call",
       0x9F: "gotoFrame2"}


def cstr(d, i):
    j = d.index(b"\0", i)
    return d[i:j].decode("latin-1"), j + 1


def disasm(d, out, ind=0, pool=None, start=0, end=None):
    pool = pool if pool is not None else []
    i = start
    end = len(d) if end is None else end
    pad = "  " * ind
    while i < end:
        op = d[i]
        at = i
        i += 1
        if op == 0:
            out.append(pad + "%05x end" % at)
            continue
        ln = 0
        if op >= 0x80:
            ln = struct.unpack_from("<H", d, i)[0]
            i += 2
        body = d[i:i + ln]
        name = OPS.get(op, "op%02x" % op)
        if op == 0x88:
            n = struct.unpack_from("<H", body)[0]
            j = 2
            pool.clear()
            for _ in range(n):
                s, j = cstr(body, j)
                pool.append(s)
            out.append(pad + "%05x constantPool[%d]" % (at, n))
        elif op == 0x96:
            vals = []
            j = 0
            while j < len(body):
                t = body[j]; j += 1
                if t == 0:
                    s, j = cstr(body, j); vals.append(repr(s))
                elif t == 1:
                    vals.append("%g" % struct.unpack_from("<f", body, j)[0]); j += 4
                elif t == 2:
                    vals.append("null")
                elif t == 3:
                    vals.append("undefined")
                elif t == 4:
                    vals.append("r%d" % body[j]); j += 1
                elif t == 5:
                    vals.append("true" if body[j] else "false"); j += 1
                elif t == 6:
                    lo, hi = struct.unpack_from("<II", body, j)
                    vals.append("%g" % struct.unpack("<d", struct.pack("<II", hi, lo))[0]); j += 8
                elif t == 7:
                    vals.append("%d" % struct.unpack_from("<i", body, j)[0]); j += 4
                elif t == 8:
                    k = body[j]; j += 1
                    vals.append(repr(pool[k]) if k < len(pool) else "c%d" % k)
                elif t == 9:
                    k = struct.unpack_from("<H", body, j)[0]; j += 2
                    vals.append(repr(pool[k]) if k < len(pool) else "c%d" % k)
                else:
                    vals.append("?%d" % t); break
            out.append(pad + "%05x push %s" % (at, ", ".join(vals)))
        elif op in (0x99, 0x9D):
            off = struct.unpack_from("<h", body)[0]
            out.append(pad + "%05x %s -> %05x" % (at, name, i + ln + off))
        elif op == 0x87:
            out.append(pad + "%05x storeRegister r%d" % (at, body[0]))
        elif op == 0x8C:
            out.append(pad + "%05x goToLabel %r" % (at, cstr(body, 0)[0]))
        elif op == 0x9F:
            out.append(pad + "%05x gotoFrame2 play=%d" % (at, body[0] & 1))
        elif op == 0x8E:
            fname, j = cstr(body, 0)
            nparams, regs, flags = struct.unpack_from("<HBH", body, j); j += 5
            params = []
            for _ in range(nparams):
                r = body[j]; j += 1
                p, j = cstr(body, j)
                params.append("%s=r%d" % (p, r) if r else p)
            size = struct.unpack_from("<H", body, j)[0]
            out.append(pad + "%05x function %s(%s) regs=%d {" % (at, fname or "<anon>", ", ".join(params), regs))
            disasm(d, out, ind + 1, pool, i + ln, i + ln + size)
            out.append(pad + "}")
            i += size
        elif op == 0x9B:
            fname, j = cstr(body, 0)
            n = struct.unpack_from("<H", body, j)[0]; j += 2
            params = []
            for _ in range(n):
                p, j = cstr(body, j); params.append(p)
            size = struct.unpack_from("<H", body, j)[0]
            out.append(pad + "%05x function %s(%s) {" % (at, fname or "<anon>", ", ".join(params)))
            disasm(d, out, ind + 1, pool, i + ln, i + ln + size)
            out.append(pad + "}")
            i += size
        else:
            out.append(pad + "%05x %s%s" % (at, name, (" " + body.hex()) if ln else ""))
        i += ln
    return out


# --- light decompiler: folds the stack into expressions; control flow stays as gotos ---

BIN = {"add": "+", "add2": "+", "subtract": "-", "multiply": "*", "divide": "/", "modulo": "%",
       "less": "<", "less2": "<", "greater": ">", "equals": "==", "equals2": "==",
       "strictEquals": "===", "and": "&&", "or": "||", "bitAnd": "&", "bitOr": "|",
       "bitXor": "^", "bitLShift": "<<", "bitRShift": ">>", "bitURShift": ">>>",
       "stringAdd": "+", "stringEquals": "==", "stringLess": "<", "instanceOf": "instanceof"}


def decompile(lines):
    """lines: output of disasm. Returns pseudo-code lines."""
    import re
    out = []
    stack = []
    regs = {}

    def pop():
        return stack.pop() if stack else "?"

    for raw in lines:
        s = raw.strip()
        ind = raw[: len(raw) - len(raw.lstrip())]
        if s == "}":
            out.append(ind + "}")
            stack = []
            continue
        m = re.match(r"([0-9a-f]{5}) (\S+)(.*)", s)
        if not m:
            out.append(raw)
            continue
        addr, op, rest = m.group(1), m.group(2), m.group(3).strip()
        if op == "push":
            # split on top-level commas (strings may contain commas)
            vals = re.findall(r"'(?:[^'\\]|\\.)*'|\"(?:[^\"\\]|\\.)*\"|[^,\s]+", rest)
            for v in vals:
                stack.append(regs.get(v, v) if re.fullmatch(r"r\d+", v) and v in regs and False else v)
        elif op == "getVariable":
            n = pop(); stack.append(n.strip("'"))
        elif op == "setVariable":
            v = pop(); n = pop(); out.append(ind + "%s %s = %s" % (addr, n.strip("'"), v))
        elif op in ("defineLocal",):
            v = pop(); n = pop(); out.append(ind + "%s var %s = %s" % (addr, n.strip("'"), v))
        elif op == "defineLocal2":
            n = pop(); out.append(ind + "%s var %s" % (addr, n.strip("'")))
        elif op == "getMember":
            n = pop(); o = pop()
            stack.append("%s.%s" % (o, n.strip("'")) if n.startswith("'") else "%s[%s]" % (o, n))
        elif op == "setMember":
            v = pop(); n = pop(); o = pop()
            out.append(ind + "%s %s.%s = %s" % (addr, o, n.strip("'"), v))
        elif op in ("callMethod", "newMethod"):
            n = pop(); o = pop(); k = pop()
            try:
                k = int(float(k))
            except Exception:
                k = 0
            args = [pop() for _ in range(k)]
            nm = n.strip("'")
            e = ("%s.%s(%s)" % (o, nm, ", ".join(args))) if nm not in ("undefined", "") else "%s(%s)" % (o, ", ".join(args))
            stack.append(("new " if op == "newMethod" else "") + e)
        elif op in ("callFunction", "newObject"):
            n = pop(); k = pop()
            try:
                k = int(float(k))
            except Exception:
                k = 0
            args = [pop() for _ in range(k)]
            stack.append(("new " if op == "newObject" else "") + "%s(%s)" % (n.strip("'"), ", ".join(args)))
        elif op in BIN:
            b = pop(); a = pop(); stack.append("(%s %s %s)" % (a, BIN[op], b))
        elif op == "not":
            stack.append("!(%s)" % pop())
        elif op in ("increment", "decrement"):
            stack.append("(%s %s 1)" % (pop(), "+" if op == "increment" else "-"))
        elif op in ("toNumber", "toInteger", "toString", "typeOf"):
            stack.append("%s(%s)" % (op, pop()))
        elif op == "pushDuplicate":
            v = pop(); stack += [v, v]
        elif op == "stackSwap":
            a = pop(); b = pop(); stack += [a, b]
        elif op == "storeRegister":
            v = stack[-1] if stack else "?"
            out.append(ind + "%s %s := %s" % (addr, rest, v))
            stack[-1:] = [rest] if stack else []
        elif op == "pop":
            v = pop()
            if "(" in v:
                out.append(ind + "%s %s" % (addr, v))
        elif op == "return":
            out.append(ind + "%s return %s" % (addr, pop()))
        elif op == "if":
            out.append(ind + "%s if %s goto %s" % (addr, pop(), rest.replace("-> ", "")))
        elif op == "jump":
            out.append(ind + "%s goto %s" % (addr, rest.replace("-> ", "")))
        elif op == "initArray":
            k = int(float(pop())); stack.append("[%s]" % ", ".join(pop() for _ in range(k)))
        elif op == "initObject":
            k = int(float(pop())); items = []
            for _ in range(k):
                v = pop(); n = pop(); items.append("%s: %s" % (n.strip("'"), v))
            stack.append("{%s}" % ", ".join(items))
        elif op == "getProperty":
            p = pop(); t = pop(); stack.append("getProperty(%s, %s)" % (t, p))
        elif op == "setProperty":
            v = pop(); p = pop(); t = pop(); out.append(ind + "%s setProperty(%s, %s, %s)" % (addr, t, p, v))
        elif op == "function":
            out.append(raw.replace(addr + " ", addr + " "))
            stack.append("<function>")
        elif op in ("trace",):
            out.append(ind + "%s trace(%s)" % (addr, pop()))
        elif op == "delete":
            n = pop(); o = pop(); out.append(ind + "%s delete %s.%s" % (addr, o, n))
        elif op == "delete2":
            out.append(ind + "%s delete %s" % (addr, pop()))
        elif op in ("extends", "implementsOp"):
            out.append(ind + "%s %s %s %s" % (addr, pop(), op, pop()))
        elif op in ("constantPool", "end"):
            pass
        elif op == "enumerate2":
            out.append(ind + "%s for-in %s" % (addr, pop())); stack.append("<enum>")
        else:
            out.append(ind + "%s %s %s" % (addr, op, rest))
    return out
