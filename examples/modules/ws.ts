export type ChatSocketContext = {
  headers: Record<string, string | undefined>;
  sendRaw: (kind: string, data: any) => void;
  sendError: (message: string) => void;
  sendMessageOut?: (data: any) => void;
  sendUserJoined?: (data: any) => void;
  sendUserLeft?: (data: any) => void;
  sendErrorMsg?: (data: any) => void;
  sendServerNotice?: (data: any) => void;
};

export async function join(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle join
}

export async function exit(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle exit
}

export async function message_in(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle message in
}

export async function typing(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle typing
}

export async function ping(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle ping (Pong is sent automatically)
}

export async function message_out(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle message out hook
}

export async function user_joined(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle user joined
}

export async function user_left(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle user left
}

export async function error(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle error
}

export async function notice(_ctx: ChatSocketContext, _data: any) {
  // TODO: handle server notice
}
