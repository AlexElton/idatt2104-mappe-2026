import { z } from "zod";

//
// Operation Schemas
//

export const OperationIdSchema = z.strictObject({
  session_id: z.string().min(1),
  replica_id: z.string().min(1),
  lamport: z.number().int().nonnegative(),
  seq: z.number().int().nonnegative(),
});

export const InsertOpSchema = z.strictObject({
  type: z.literal("Insert"),
  left: OperationIdSchema.nullable(),
  value: z.string().refine((value) => Array.from(value).length === 1, {
    message: "Insert operations must contain exactly one Unicode character",
  }),
  id: OperationIdSchema,
});

export const DeleteOpSchema = z.strictObject({
  type: z.literal("Delete"),
  target: OperationIdSchema,
  id: OperationIdSchema,
});

export const OpSchema = z.discriminatedUnion("type", [InsertOpSchema, DeleteOpSchema]);

export const PresenceSchema = z.strictObject({
  replica_id: z.string().min(1),
  cursor: z.number().int().nonnegative(),
});

const PresenceByReplicaSchema = z.record(z.string(), PresenceSchema);

//
// Message Schemas
//

export const HelloMsgSchema = z.strictObject({
  type: z.literal("hello"),
  replica_id: z.string().min(1),
  session_id: z.string().min(1),
});

export const ClientOpsMsgSchema = z.strictObject({
  type: z.literal("ops"),
  ops: z.array(OpSchema),
});

export const ClientPresenceMsgSchema = z.strictObject({
  type: z.literal("presence"),
  presence: PresenceSchema,
});

export const ClientGarbageCollectMsgSchema = z.strictObject({
  type: z.literal("garbage_collect"),
});

export const ClientMsgSchema = z.discriminatedUnion("type", [
  HelloMsgSchema,
  ClientOpsMsgSchema,
  ClientPresenceMsgSchema,
  ClientGarbageCollectMsgSchema,
]);

export const HydrateMsgSchema = z.strictObject({
  type: z.literal("hydrate"),
  ops: z.array(OpSchema),
  presence: PresenceByReplicaSchema,
  clients: z.number().int().nonnegative(),
});

export const RemoteOpsMsgSchema = z.strictObject({
  type: z.literal("ops"),
  ops: z.array(OpSchema),
});

export const PresenceStateMsgSchema = z.strictObject({
  type: z.literal("presence"),
  presence: PresenceByReplicaSchema,
  clients: z.number().int().nonnegative(),
});

export const GarbageCollectMsgSchema = z.strictObject({
  type: z.literal("garbage_collect"),
  removed: z.number().int().nonnegative(),
});

export const ServerMsgSchema = z.discriminatedUnion("type", [
  HydrateMsgSchema,
  RemoteOpsMsgSchema,
  PresenceStateMsgSchema,
  GarbageCollectMsgSchema,
]);

//
// Types
//

export type OperationId = z.infer<typeof OperationIdSchema>;
export type Op = z.infer<typeof OpSchema>;
export type Presence = z.infer<typeof PresenceSchema>;
export type ClientMsg = z.infer<typeof ClientMsgSchema>;
export type ServerMsg = z.infer<typeof ServerMsgSchema>;
