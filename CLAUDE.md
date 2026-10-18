dont touch rust_distributable thats a separate project. 


any changes made inside rust_distributable require you to make the neessary problem md updates, and architecture if its change remain modular and clean. so future changes require minimum context window. 



if you need latest session logs it is exactly here *barely needed*
Export visible (raw)

What it writes: a new button under the session list writes the events the timeline shows right now, using your type filters and search, as raw text, one event per line.
Where it goes: web_data/exports/<session>_<filters>.txt, plus a copy at exports/latest.txt that always has the same path. The path is also copied to your clipboard.
Download: the same text is available from GET /api/sessions/<id>/raw.
Checked on a real session: I exported one unfiltered and got 19 raw lines. A line break inside a frame is written as \n so one event stays one line.
