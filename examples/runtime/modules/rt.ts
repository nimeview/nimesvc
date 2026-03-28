let count = 0;

export function health(): string {
  return "ok";
}

export function createUser(name: string, email: string) {
  count += 1;
  return { id: 1, name, email };
}

export function getCount(): number {
  return count;
}

export function wsJoin(ctx: any, _data: any): void {
  if (ctx && typeof ctx.sendRaw === "function") {
    ctx.sendRaw("MessageOut", { text: "joined" });
  }
}

export function wsExit(_ctx: any, _data: any): void {
  // TODO
}

export function wsMessageIn(ctx: any, data: any): void {
  if (ctx && typeof ctx.sendRaw === "function") {
    ctx.sendRaw("MessageOut", data);
  }
}

export function wsPing(_ctx: any, _data: any): void {
  // Pong is automatic
}

export function wsMessageOut(_ctx: any, _data: any): void {}
export function wsError(_ctx: any, _data: any): void {}
