// Spike S2.7 (docs/spikes.md): the DNS messages in a pcapng capture, for
// s2-7-egress.ps1 on Windows. There, WFP's logs name each connection's
// process but not the host it looked up, and WebView2's network service
// sends its own DNS queries, which the DNS client's log never sees. pktmon
// captures the packets; this reads the DNS ones, so that a connection's
// local port or remote address can be matched with a name.
//
// Usage: node scripts/spikes/s2-7-dns.mjs <capture.pcapng>
// Prints a JSON array, one object per DNS message over UDP:
//   { time, src, sport, dst, dport, id, response, rcode, questions: [{ name, type }],
//     answers: [{ name, type, data }] }
// time is in seconds since the epoch. Ethernet (with or without a VLAN tag)
// and raw IP frames are read; other link types, TCP DNS and frames cut short
// are skipped.
import { readFileSync } from 'node:fs';

const TYPES = {
  1: 'A',
  2: 'NS',
  5: 'CNAME',
  6: 'SOA',
  12: 'PTR',
  16: 'TXT',
  28: 'AAAA',
  33: 'SRV',
  64: 'SVCB',
  65: 'HTTPS',
};

const file = process.argv[2];
if (!file) {
  console.error('usage: node s2-7-dns.mjs <capture.pcapng>');
  process.exit(2);
}
const buf = readFileSync(file);

/** Per interface: link type and timestamp units per second. */
const interfaces = [];
const messages = [];
let little = true;

const u16 = (offset) => (little ? buf.readUInt16LE(offset) : buf.readUInt16BE(offset));
const u32 = (offset) => (little ? buf.readUInt32LE(offset) : buf.readUInt32BE(offset));

function ipv4(bytes, at) {
  return `${bytes[at]}.${bytes[at + 1]}.${bytes[at + 2]}.${bytes[at + 3]}`;
}

function ipv6(bytes, at) {
  const groups = [];
  for (let i = 0; i < 16; i += 2)
    groups.push(((bytes[at + i] << 8) | bytes[at + i + 1]).toString(16));
  // The longest run of zero groups becomes "::".
  let best = -1;
  let length = 0;
  for (let i = 0; i < 8;) {
    if (groups[i] !== '0') {
      i++;
      continue;
    }
    let j = i;
    while (j < 8 && groups[j] === '0') j++;
    if (j - i > length && j - i > 1) {
      best = i;
      length = j - i;
    }
    i = j;
  }
  if (best < 0) return groups.join(':');
  return `${groups.slice(0, best).join(':')}::${groups.slice(best + length).join(':')}`;
}

/** A DNS name at `at` in `msg`, following compression pointers. */
function readName(msg, at) {
  const labels = [];
  let jumps = 0;
  let next = -1;
  for (;;) {
    if (at >= msg.length) throw new Error('name runs past the message');
    const len = msg[at];
    if (len === 0) {
      at++;
      break;
    }
    if ((len & 0xc0) === 0xc0) {
      if (++jumps > 20) throw new Error('compression loop');
      if (next < 0) next = at + 2;
      at = ((len & 0x3f) << 8) | msg[at + 1];
      continue;
    }
    labels.push(msg.subarray(at + 1, at + 1 + len).toString('latin1'));
    at += 1 + len;
  }
  return { name: labels.join('.') || '.', end: next < 0 ? at : next };
}

function parseDns(msg) {
  if (msg.length < 12) return null;
  const flags = msg.readUInt16BE(2);
  const counts = [msg.readUInt16BE(4), msg.readUInt16BE(6)];
  const out = {
    id: msg.readUInt16BE(0),
    response: (flags & 0x8000) !== 0,
    rcode: flags & 0xf,
    questions: [],
    answers: [],
  };
  let at = 12;
  try {
    for (let i = 0; i < counts[0]; i++) {
      const { name, end } = readName(msg, at);
      const type = msg.readUInt16BE(end);
      out.questions.push({ name, type: TYPES[type] ?? String(type) });
      at = end + 4;
    }
    for (let i = 0; i < counts[1]; i++) {
      const { name, end } = readName(msg, at);
      const type = msg.readUInt16BE(end);
      const rdlength = msg.readUInt16BE(end + 8);
      const rdata = end + 10;
      let data = '';
      if (type === 1 && rdlength === 4) data = ipv4(msg, rdata);
      else if (type === 28 && rdlength === 16) data = ipv6(msg, rdata);
      else if (type === 5 || type === 12) data = readName(msg, rdata).name;
      out.answers.push({ name, type: TYPES[type] ?? String(type), data });
      at = rdata + rdlength;
    }
  } catch {
    // A message cut by the snap length keeps what was read.
  }
  return out;
}

function onFrame(linkType, time, frame) {
  let at = 0;
  let ethertype;
  if (linkType === 1) {
    if (frame.length < 14) return;
    ethertype = frame.readUInt16BE(12);
    at = 14;
    while (ethertype === 0x8100 || ethertype === 0x88a8) {
      ethertype = frame.readUInt16BE(at + 2);
      at += 4;
    }
  } else if (linkType === 101 || linkType === 12 || linkType === 228 || linkType === 229) {
    ethertype = frame[0] >> 4 === 6 ? 0x86dd : 0x0800;
  } else {
    return;
  }
  let src;
  let dst;
  let proto;
  if (ethertype === 0x0800) {
    const ihl = (frame[at] & 0xf) * 4;
    proto = frame[at + 9];
    src = ipv4(frame, at + 12);
    dst = ipv4(frame, at + 16);
    at += ihl;
  } else if (ethertype === 0x86dd) {
    proto = frame[at + 6];
    src = ipv6(frame, at + 8);
    dst = ipv6(frame, at + 24);
    at += 40;
  } else {
    return;
  }
  if (proto !== 17 || at + 8 > frame.length) return;
  const sport = frame.readUInt16BE(at);
  const dport = frame.readUInt16BE(at + 2);
  if (sport !== 53 && dport !== 53) return;
  const dns = parseDns(frame.subarray(at + 8));
  if (dns) messages.push({ time, src, sport, dst, dport, ...dns });
}

let at = 0;
while (at + 12 <= buf.length) {
  const type = buf.readUInt32LE(at);
  if (type === 0x0a0d0d0a) {
    const magic = buf.readUInt32LE(at + 8);
    little = magic === 0x1a2b3c4d;
  }
  const length = u32(at + 4);
  if (length < 12 || at + length > buf.length) break;
  const body = at + 8;
  if (type === 0x00000001) {
    const linkType = u16(body);
    let perSecond = 1e6;
    // Options after link type (2), reserved (2) and snap length (4).
    let option = body + 8;
    while (option + 4 <= at + length - 4) {
      const code = u16(option);
      const size = u16(option + 2);
      if (code === 0) break;
      if (code === 9 && size >= 1) {
        const value = buf[option + 4];
        perSecond = value & 0x80 ? 2 ** (value & 0x7f) : 10 ** value;
      }
      option += 4 + Math.ceil(size / 4) * 4;
    }
    interfaces.push({ linkType, perSecond });
  } else if (type === 0x00000006) {
    const iface = interfaces[u32(body)];
    const stamp = (BigInt(u32(body + 4)) << 32n) | BigInt(u32(body + 8));
    const captured = u32(body + 12);
    if (iface) {
      const time = Number(stamp) / iface.perSecond;
      onFrame(iface.linkType, time, buf.subarray(body + 20, body + 20 + captured));
    }
  } else if (type === 0x00000003 && interfaces[0]) {
    const captured = Math.min(u32(body), length - 16);
    onFrame(interfaces[0].linkType, 0, buf.subarray(body + 4, body + 4 + captured));
  }
  at += length;
}

process.stdout.write(`${JSON.stringify(messages)}\n`);
