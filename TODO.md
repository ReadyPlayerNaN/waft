## 1. Sunsetr hangs the application

Setting: During the daylight (before sunsetr switches to night light), while sunsetr is running
Action: Click on the Night light feature toggle
Result: The application hangs, UI is no longer responsive
Memory dump -> diagnose-cpu-20260202-135933.log

### Expected result

The plugin behaviour is wrong. The Feature Toggle should display as "on", whenever the sunsetr is running. When the sunsetr shuts down (or is not running), the feature toggle should display as "off". Clicking feature toggle that is "off" should check if sunsetr is running, if yes, then just update the status to plugin state; if not, then it should start sunsetr and propagate the status to plugin state.

## 2. Sunsetr label

The sunsetr feature toggle should display "Denní režim do {čas}" and "Noční světlo do {čas}", based on the sunsetr period.

## 3. Universal Feature toggle component

There should be only a single Feature Toggle component. The Feature Toggle and Extendable Feature Toggle must be merged together. Internally, there must still be two variants, identical to the current two variants, but the UI must allow to switch between them. I think the best way to do this is to resolve this exclusively by CSS styling = both variants would render

```
<gtk::Box>
  <MainButton />
  <ExpandButton />
</gtk::Box>
```

But only the expandable case would receive CSS class "expandable".

## 4. Sunsetr options

The sunsetr feature toggle must be expandable, but only if the sunsetr is running. If it is running, it must provide menu, that allows switching sunsetr presets.

## 5. Plugins to implement

- Tether plugin?
- SNI

## 6. NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
