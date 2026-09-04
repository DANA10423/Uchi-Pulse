use heapless::Vec;
use uchi_pulse_common::{ActionId, InputEvent};

pub const MAX_INPUT_CHANNELS: usize = 32;
pub const MAX_EVENTS_PER_SAMPLE: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputConfig<'a> {
    pub gpio_inputs: &'a [crate::config::GpioInputConfig],
    pub mappings: &'a [crate::config::InputMapping],
    pub double_click_interval_ms: u32,
    pub long_press_threshold_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DetectedInputEvent {
    pub gpio: u8,
    pub input_event: InputEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggeredAction {
    pub action_id: ActionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputSampleResult {
    pub input_events: Vec<DetectedInputEvent, MAX_EVENTS_PER_SAMPLE>,
    pub actions: Vec<TriggeredAction, MAX_EVENTS_PER_SAMPLE>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputConfigError {
    TooManyInputs,
    DuplicateGpio(u8),
    MappingReferencesUnknownGpio(u8),
    DuplicateMapping { gpio: u8, input_event: InputEvent },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputProcessError {
    UnknownGpio(u8),
    TooManyEvents,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GestureDetector {
    stable: bool,
    candidate: bool,
    candidate_since_ms: u64,
    pressed_since_ms: Option<u64>,
    long_press_emitted: bool,
    pending_click_since_ms: Option<u64>,
    second_click_in_progress: bool,
}

impl GestureDetector {
    const fn new() -> Self {
        Self {
            stable: false,
            candidate: false,
            candidate_since_ms: 0,
            pressed_since_ms: None,
            long_press_emitted: false,
            pending_click_since_ms: None,
            second_click_in_progress: false,
        }
    }

    fn initialize(&mut self, active: bool, now_ms: u64) {
        self.stable = active;
        self.candidate = active;
        self.candidate_since_ms = now_ms;
        self.pressed_since_ms = active.then_some(now_ms);
        self.long_press_emitted = false;
        self.pending_click_since_ms = None;
        self.second_click_in_progress = false;
    }

    fn update(
        &mut self,
        active: bool,
        now_ms: u64,
        debounce_ms: u16,
        double_click_interval_ms: u32,
        long_press_threshold_ms: u32,
        output: &mut Vec<InputEvent, MAX_EVENTS_PER_SAMPLE>,
    ) {
        if let Some(pending_since_ms) = self.pending_click_since_ms {
            let second_press_started_in_window = active
                && self.candidate != self.stable
                && self.candidate
                && elapsed(self.candidate_since_ms, pending_since_ms)
                    <= u64::from(double_click_interval_ms);
            if elapsed(now_ms, pending_since_ms) > u64::from(double_click_interval_ms)
                && !second_press_started_in_window
            {
                push_event(output, InputEvent::Click);
                self.pending_click_since_ms = None;
                self.second_click_in_progress = false;
            }
        }

        if active != self.candidate {
            self.candidate = active;
            self.candidate_since_ms = now_ms;
        } else if self.candidate != self.stable
            && elapsed(now_ms, self.candidate_since_ms) >= u64::from(debounce_ms)
        {
            self.stable = self.candidate;
            if self.stable {
                push_event(output, InputEvent::OffToOn);
                self.pressed_since_ms = Some(now_ms);
                self.long_press_emitted = false;
                self.second_click_in_progress = self.pending_click_since_ms.is_some_and(|since| {
                    elapsed(self.candidate_since_ms, since) <= u64::from(double_click_interval_ms)
                });
                if self.second_click_in_progress {
                    self.pending_click_since_ms = None;
                }
            } else {
                push_event(output, InputEvent::OnToOff);
                if self.long_press_emitted {
                    self.pressed_since_ms = None;
                    self.second_click_in_progress = false;
                } else if self.second_click_in_progress {
                    push_event(output, InputEvent::DoubleClick);
                    self.pending_click_since_ms = None;
                    self.second_click_in_progress = false;
                    self.pressed_since_ms = None;
                } else {
                    self.pending_click_since_ms = Some(now_ms);
                    self.pressed_since_ms = None;
                }
            }
        }

        if self.stable
            && !self.long_press_emitted
            && let Some(pressed_since_ms) = self.pressed_since_ms
            && elapsed(now_ms, pressed_since_ms) >= u64::from(long_press_threshold_ms)
        {
            push_event(output, InputEvent::LongPress);
            self.long_press_emitted = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeInput {
    gpio: u8,
    active_high: bool,
    debounce_ms: u16,
    detector: GestureDetector,
}

pub struct InputController<'a> {
    mappings: &'a [crate::config::InputMapping],
    double_click_interval_ms: u32,
    long_press_threshold_ms: u32,
    inputs: Vec<RuntimeInput, MAX_INPUT_CHANNELS>,
}

impl<'a> InputController<'a> {
    pub fn new(config: InputConfig<'a>) -> Result<Self, InputConfigError> {
        let mut inputs: Vec<RuntimeInput, MAX_INPUT_CHANNELS> = Vec::new();
        for gpio_input in config.gpio_inputs {
            if inputs.iter().any(|input| input.gpio == gpio_input.gpio) {
                return Err(InputConfigError::DuplicateGpio(gpio_input.gpio));
            }
            inputs
                .push(RuntimeInput {
                    gpio: gpio_input.gpio,
                    active_high: gpio_input.active_high,
                    debounce_ms: gpio_input.debounce_ms,
                    detector: GestureDetector::new(),
                })
                .map_err(|_| InputConfigError::TooManyInputs)?;
        }

        for (index, mapping) in config.mappings.iter().enumerate() {
            if !inputs.iter().any(|input| input.gpio == mapping.gpio) {
                return Err(InputConfigError::MappingReferencesUnknownGpio(mapping.gpio));
            }
            if config
                .mappings
                .iter()
                .skip(index + 1)
                .any(|other| other.gpio == mapping.gpio && other.input_event == mapping.input_event)
            {
                return Err(InputConfigError::DuplicateMapping {
                    gpio: mapping.gpio,
                    input_event: mapping.input_event,
                });
            }
        }

        Ok(Self {
            mappings: config.mappings,
            double_click_interval_ms: config.double_click_interval_ms,
            long_press_threshold_ms: config.long_press_threshold_ms,
            inputs,
        })
    }

    pub fn set_gesture_timing(
        &mut self,
        double_click_interval_ms: u32,
        long_press_threshold_ms: u32,
    ) {
        self.double_click_interval_ms = double_click_interval_ms;
        self.long_press_threshold_ms = long_press_threshold_ms;
    }

    pub fn has_gpio(&self, gpio: u8) -> bool {
        self.inputs.iter().any(|input| input.gpio == gpio)
    }

    pub fn gesture_timing(&self) -> (u32, u32) {
        (self.double_click_interval_ms, self.long_press_threshold_ms)
    }

    pub fn initialize(
        &mut self,
        gpio: u8,
        raw_high: bool,
        now_ms: u64,
    ) -> Result<(), InputProcessError> {
        let input = self
            .inputs
            .iter_mut()
            .find(|input| input.gpio == gpio)
            .ok_or(InputProcessError::UnknownGpio(gpio))?;
        input.detector.initialize(
            if input.active_high {
                raw_high
            } else {
                !raw_high
            },
            now_ms,
        );
        Ok(())
    }

    pub fn update(
        &mut self,
        gpio: u8,
        raw_high: bool,
        now_ms: u64,
    ) -> Result<InputSampleResult, InputProcessError> {
        let input = self
            .inputs
            .iter_mut()
            .find(|input| input.gpio == gpio)
            .ok_or(InputProcessError::UnknownGpio(gpio))?;
        let active = if input.active_high {
            raw_high
        } else {
            !raw_high
        };
        let mut detected = Vec::new();
        input.detector.update(
            active,
            now_ms,
            input.debounce_ms,
            self.double_click_interval_ms,
            self.long_press_threshold_ms,
            &mut detected,
        );

        let mut input_events = Vec::new();
        let mut actions = Vec::new();
        for input_event in detected {
            input_events
                .push(DetectedInputEvent { gpio, input_event })
                .map_err(|_| InputProcessError::TooManyEvents)?;
            for mapping in self.mappings.iter().filter(|mapping| {
                mapping.enabled && mapping.gpio == gpio && mapping.input_event == input_event
            }) {
                actions
                    .push(TriggeredAction {
                        action_id: mapping.action_id,
                    })
                    .map_err(|_| InputProcessError::TooManyEvents)?;
            }
        }

        Ok(InputSampleResult {
            input_events,
            actions,
        })
    }
}

fn elapsed(now_ms: u64, since_ms: u64) -> u64 {
    now_ms.saturating_sub(since_ms)
}

fn push_event(output: &mut Vec<InputEvent, MAX_EVENTS_PER_SAMPLE>, event: InputEvent) {
    let _ = output.push(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GpioInputConfig, InputMapping};

    const GPIO: u8 = 5;

    const GPIO_INPUTS: &[GpioInputConfig] = &[GpioInputConfig {
        gpio: GPIO,
        active_high: true,
        debounce_ms: 30,
    }];

    fn controller<'a>(mappings: &'a [InputMapping]) -> InputController<'a> {
        InputController::new(InputConfig {
            gpio_inputs: GPIO_INPUTS,
            mappings,
            double_click_interval_ms: 400,
            long_press_threshold_ms: 1_000,
        })
        .unwrap()
    }

    fn sample<'a>(
        controller: &mut InputController<'a>,
        active: bool,
        now_ms: u64,
    ) -> InputSampleResult {
        controller.update(GPIO, active, now_ms).unwrap()
    }

    fn initialize<'a>(controller: &mut InputController<'a>) {
        controller.initialize(GPIO, false, 0).unwrap();
    }

    #[test]
    fn stable_edges_emit_once_after_debounce() {
        let mappings = [
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::OffToOn,
                action_id: 10,
                enabled: true,
            },
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::OnToOff,
                action_id: 11,
                enabled: true,
            },
        ];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        assert!(sample(&mut controller, true, 1).input_events.is_empty());
        assert_eq!(
            sample(&mut controller, true, 31).input_events.as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::OffToOn
            }]
        );
        assert!(sample(&mut controller, true, 100).input_events.is_empty());
        assert_eq!(
            sample(&mut controller, false, 101).input_events.as_slice(),
            &[]
        );
        assert_eq!(
            sample(&mut controller, false, 131).input_events.as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::OnToOff
            }]
        );
        assert!(sample(&mut controller, false, 200).input_events.is_empty());
    }

    #[test]
    fn debounce_rejects_chattering() {
        let mappings = [InputMapping {
            gpio: GPIO,
            input_event: InputEvent::OffToOn,
            action_id: 10,
            enabled: true,
        }];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        for (active, now_ms) in [(true, 1), (false, 10), (true, 20), (false, 29), (false, 50)] {
            assert!(
                sample(&mut controller, active, now_ms)
                    .input_events
                    .is_empty()
            );
        }
    }

    #[test]
    fn short_press_emits_click_only_after_double_click_window() {
        let mappings = [InputMapping {
            gpio: GPIO,
            input_event: InputEvent::Click,
            action_id: 10,
            enabled: true,
        }];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        sample(&mut controller, false, 32);
        sample(&mut controller, false, 62);
        assert!(sample(&mut controller, false, 462).input_events.is_empty());
        let result = sample(&mut controller, false, 463);
        assert_eq!(
            result.input_events.as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::Click
            }]
        );
        assert_eq!(
            result.actions.as_slice(),
            &[TriggeredAction { action_id: 10 }]
        );
    }

    #[test]
    fn two_short_presses_emit_double_click_without_clicks() {
        let mappings = [
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::Click,
                action_id: 10,
                enabled: true,
            },
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::DoubleClick,
                action_id: 20,
                enabled: true,
            },
        ];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        sample(&mut controller, false, 32);
        sample(&mut controller, false, 62);
        sample(&mut controller, true, 100);
        sample(&mut controller, true, 130);
        sample(&mut controller, false, 131);
        let result = sample(&mut controller, false, 161);
        assert_eq!(
            result.input_events.as_slice(),
            &[
                DetectedInputEvent {
                    gpio: GPIO,
                    input_event: InputEvent::OnToOff
                },
                DetectedInputEvent {
                    gpio: GPIO,
                    input_event: InputEvent::DoubleClick
                }
            ]
        );
        assert_eq!(
            result.actions.as_slice(),
            &[TriggeredAction { action_id: 20 }]
        );
        assert!(sample(&mut controller, false, 500).input_events.is_empty());
    }

    #[test]
    fn a_second_press_keeps_double_click_pending_while_held() {
        let mappings = [
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::Click,
                action_id: 10,
                enabled: true,
            },
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::DoubleClick,
                action_id: 20,
                enabled: true,
            },
        ];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        sample(&mut controller, false, 32);
        sample(&mut controller, false, 62);
        sample(&mut controller, true, 100);
        sample(&mut controller, true, 130);
        assert!(sample(&mut controller, true, 600).input_events.is_empty());
        sample(&mut controller, false, 601);
        let result = sample(&mut controller, false, 631);
        assert_eq!(
            result.input_events.as_slice(),
            &[
                DetectedInputEvent {
                    gpio: GPIO,
                    input_event: InputEvent::OnToOff
                },
                DetectedInputEvent {
                    gpio: GPIO,
                    input_event: InputEvent::DoubleClick
                }
            ]
        );
        assert_eq!(
            result.actions.as_slice(),
            &[TriggeredAction { action_id: 20 }]
        );
    }

    #[test]
    fn second_press_is_measured_from_physical_start_before_debounce() {
        let mappings = [InputMapping {
            gpio: GPIO,
            input_event: InputEvent::DoubleClick,
            action_id: 20,
            enabled: true,
        }];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        sample(&mut controller, false, 32);
        sample(&mut controller, false, 62);
        sample(&mut controller, true, 400);
        sample(&mut controller, true, 430);
        sample(&mut controller, false, 431);
        let result = sample(&mut controller, false, 461);
        assert!(result.input_events.contains(&DetectedInputEvent {
            gpio: GPIO,
            input_event: InputEvent::DoubleClick
        }));
        assert_eq!(
            result.actions.as_slice(),
            &[TriggeredAction { action_id: 20 }]
        );
    }

    #[test]
    fn long_press_emits_once_and_never_becomes_click() {
        let mappings = [
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::LongPress,
                action_id: 30,
                enabled: true,
            },
            InputMapping {
                gpio: GPIO,
                input_event: InputEvent::Click,
                action_id: 10,
                enabled: true,
            },
        ];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        let result = sample(&mut controller, true, 1_031);
        assert_eq!(
            result.input_events.as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::LongPress
            }]
        );
        assert_eq!(
            result.actions.as_slice(),
            &[TriggeredAction { action_id: 30 }]
        );
        assert!(sample(&mut controller, true, 2_000).input_events.is_empty());
        assert!(
            sample(&mut controller, false, 2_001)
                .input_events
                .is_empty()
        );
        assert_eq!(
            sample(&mut controller, false, 2_031)
                .input_events
                .as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::OnToOff
            }]
        );
        assert!(
            sample(&mut controller, false, 2_100)
                .input_events
                .is_empty()
        );
    }

    #[test]
    fn configured_timing_is_used_and_can_be_changed() {
        let mappings = [InputMapping {
            gpio: GPIO,
            input_event: InputEvent::LongPress,
            action_id: 30,
            enabled: true,
        }];
        let mut controller = controller(&mappings);
        assert_eq!(controller.gesture_timing(), (400, 1_000));
        controller.set_gesture_timing(100, 50);
        assert_eq!(controller.gesture_timing(), (100, 50));
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        assert!(
            sample(&mut controller, true, 81)
                .input_events
                .contains(&DetectedInputEvent {
                    gpio: GPIO,
                    input_event: InputEvent::LongPress,
                })
        );
    }

    #[test]
    fn missing_or_disabled_mapping_does_not_emit_action() {
        let mappings = [InputMapping {
            gpio: GPIO,
            input_event: InputEvent::Click,
            action_id: 10,
            enabled: false,
        }];
        let mut controller = controller(&mappings);
        initialize(&mut controller);
        sample(&mut controller, true, 1);
        sample(&mut controller, true, 31);
        sample(&mut controller, false, 32);
        sample(&mut controller, false, 62);
        let result = sample(&mut controller, false, 463);
        assert_eq!(
            result.input_events.as_slice(),
            &[DetectedInputEvent {
                gpio: GPIO,
                input_event: InputEvent::Click
            }]
        );
        assert!(result.actions.is_empty());
    }

    #[test]
    fn multiple_gpios_and_events_are_resolved_independently() {
        let gpio_inputs = [
            GpioInputConfig {
                gpio: 5,
                active_high: true,
                debounce_ms: 0,
            },
            GpioInputConfig {
                gpio: 6,
                active_high: true,
                debounce_ms: 0,
            },
        ];
        let mappings = [
            InputMapping {
                gpio: 5,
                input_event: InputEvent::OffToOn,
                action_id: 50,
                enabled: true,
            },
            InputMapping {
                gpio: 6,
                input_event: InputEvent::OffToOn,
                action_id: 60,
                enabled: true,
            },
        ];
        let mut controller = InputController::new(InputConfig {
            gpio_inputs: &gpio_inputs,
            mappings: &mappings,
            double_click_interval_ms: 400,
            long_press_threshold_ms: 1_000,
        })
        .unwrap();
        controller.initialize(5, false, 0).unwrap();
        controller.initialize(6, false, 0).unwrap();
        controller.update(5, true, 1).unwrap();
        controller.update(6, true, 2).unwrap();
        assert_eq!(
            controller.update(5, true, 3).unwrap().actions.as_slice(),
            &[TriggeredAction { action_id: 50 }]
        );
        assert_eq!(
            controller.update(6, true, 4).unwrap().actions.as_slice(),
            &[TriggeredAction { action_id: 60 }]
        );
    }
}
