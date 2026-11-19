// What a control and the `Field` around it tell each other (ADR-0042): the
// control's id, for the label; what describes it, a hint or an error; and
// whether it is in error. A control outside a `Field` has its own.
import {
  computed,
  inject,
  provide,
  useId,
  type ComputedRef,
  type InjectionKey,
  type Ref,
} from "vue";

export interface FieldContext {
  /** The control's id, which the label points at. */
  id: string;
  /** The label's id, for a control that is a group and can't be pointed at. */
  labelId: string;
  /** The id of the hint or the error shown, if one is. */
  describedBy: ComputedRef<string | undefined>;
  /** Whether the field has an error. */
  invalid: ComputedRef<boolean>;
}

const FIELD: InjectionKey<FieldContext> = Symbol("field");

/** Makes a `Field` known to the control inside it. */
export function provideField(field: FieldContext): void {
  provide(FIELD, field);
}

/**
 * Whether the controls in a part of the page are locked: the app read-only,
 * without the control lease (ADR-0048). A part inside may free them again.
 */
const LOCKED: InjectionKey<Readonly<Ref<boolean>>> = Symbol("locked");

/** Locks, or frees, the controls inside the component that calls it. */
export function provideLocked(locked: Readonly<Ref<boolean>>): void {
  provide(LOCKED, locked);
}

/** Whether a control is switched off: by its own `disabled`, or by a locked part of the page. */
export function useDisabled(props: { disabled?: boolean | undefined }): ComputedRef<boolean> {
  const locked = inject(LOCKED, null);
  return computed(() => Boolean(props.disabled) || (locked?.value ?? false));
}

/** What every control takes. */
export interface ControlProps {
  /** Its id, where no `Field` gives it one. */
  id?: string;
  /** Its name, where no `Field` and no text beside it gives it one. */
  label?: string;
  /** Switched off: it can't be used, and the keyboard passes it by. */
  disabled?: boolean;
  /** Shown, and reachable, but not to be changed. */
  readonly?: boolean;
  /** In error, where no `Field` says so. */
  invalid?: boolean;
}

/** A control's id and state: its own, or those of the `Field` around it. */
export function useControl(props: ControlProps) {
  const field = inject(FIELD, null);
  const own = useId();
  return {
    id: computed(() => props.id ?? field?.id ?? own),
    labelledBy: field?.labelId,
    describedBy: computed(() => field?.describedBy.value),
    invalid: computed(() => Boolean(props.invalid) || (field?.invalid.value ?? false)),
    /** Switched off: by its own prop, or by the locked part of the page it is in. */
    disabled: useDisabled(props),
  };
}
