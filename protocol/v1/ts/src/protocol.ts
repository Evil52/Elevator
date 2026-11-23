export type Message =
  | {
      hello: Hello;
    }
  | {
      carSnapshot: CarSnapshot;
    }
  | {
      callFloor: CallFloor;
    }
  | {
      commandResult: CommandResult;
    }
  | {
      alarmPage: AlarmPage;
    }
  | {
      ackAlarm: AckAlarm;
    }
  | {
      injectFault: InjectFault;
    }
  | {
      recoverCar: RecoverCar;
    }
  | {
      simControl: SimControl;
    }
  | {
      eventLogChunk: EventLogChunk;
    }
  | {
      readLog: ReadLog;
    }
  | {
      ping: Ping;
    };
export type ResetReason = "powerOn" | "button" | "watchdog" | "software" | "brownout";
export type DoorState = "closed" | "opening" | "open" | "closing";
export type CarMode = "normal" | "fireRecall" | "firefighters" | "unavailable";
export type Direction = "up" | "down";
export type CommandStatus =
  | ("accepted" | "applied" | "expired" | "unknown")
  | {
      rejected: Rejection;
    };
export type Rejection =
  "noSuchFloor" | "downFromFirst" | "upFromTop" | "wrongBoot" | "wrongRun" | "noSuchAlarm" | "noSuchCar" | "wrongState";
export type AlarmKind = "fireRecall" | "carFault" | "linkLost" | "logOverflow";
export type AlarmState = "active" | "acknowledged" | "cleared";
export type SimAction = "pause" | "resume" | "reset" | "fireRecall" | "clearFire";
export type EventKind =
  "assigned" | "arrived" | "served" | "requeued" | "cancelled" | "alarmRaised" | "alarmAcked" | "alarmCleared";

export interface Envelope {
  bootId: number;
  message: Message;
  runId: number;
  seq: number;
  version: 1;
}
export interface Hello {
  deviceId: number;
  resetReason: ResetReason;
}
export interface CarSnapshot {
  car: number;
  doors: DoorState;
  downCalls: number;
  etaMs?: number | null;
  floor?: number | null;
  mode: CarMode;
  modelTimeMs: number;
  positionMm: number;
  speedMmPerSec: number;
  upCalls: number;
}
export interface CallFloor {
  direction: Direction;
  floor: number;
  requestId: number;
}
export interface CommandResult {
  requestId: number;
  status: CommandStatus;
}
export interface AlarmPage {
  /**
   * @minItems 8
   * @maxItems 8
   */
  items: [
    Alarm | null,
    Alarm | null,
    Alarm | null,
    Alarm | null,
    Alarm | null,
    Alarm | null,
    Alarm | null,
    Alarm | null
  ];
  offset: number;
  total: number;
}
export interface Alarm {
  car?: number | null;
  id: number;
  kind: AlarmKind;
  raisedAtMs: number;
  state: AlarmState;
}
export interface AckAlarm {
  alarmId: number;
  requestId: number;
}
export interface InjectFault {
  car: number;
  requestId: number;
}
export interface RecoverCar {
  car: number;
  requestId: number;
}
export interface SimControl {
  action: SimAction;
  requestId: number;
}
export interface EventLogChunk {
  droppedBefore: number;
  /**
   * @minItems 8
   * @maxItems 8
   */
  events: [
    Event | null,
    Event | null,
    Event | null,
    Event | null,
    Event | null,
    Event | null,
    Event | null,
    Event | null
  ];
  firstIndex: number;
}
export interface Event {
  atMs: number;
  car?: number | null;
  id: number;
  index: number;
  kind: EventKind;
}
export interface ReadLog {
  fromIndex: number;
  requestId: number;
}
export interface Ping {
  counter: number;
}
