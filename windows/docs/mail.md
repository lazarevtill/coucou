# Mail with classic Outlook

Turn the **Mail** pill on in Notch → Active pills. Coucou reads the Outlook already open on
your PC — the classic Outlook of Microsoft 365, through its own COM objects — with no sign-in,
no app to register at Microsoft and no admin consent. It never starts Outlook, and a closed
Outlook costs nothing. The new Outlook for Windows has no COM objects, so it is not reached
this way. Linux has no Outlook: the pill is Windows only.

| What you do | What happens |
|---|---|
| Open the Mail pill | Your newest messages, unread ones tinted, a ⚑ on the flagged ones. The magnifier searches every inbox: free words, `from:anne`, `subject:"May quote"`. Filters: All, Unread, Important, Today |
| Click a message | Its sender, subject and the start of its text. **Open in Outlook**, **Reply** (Outlook's own window: nothing is sent from Coucou), **Mark as read**, **Important**, **Junk**, and in ⋯ **Always important (VIP)** or **Always ignore this sender** |
| Coucou → Mail → Overview | **Sort new messages automatically** (off by default): only what arrives after you turn it on, each message once. VIPs are flagged, banned words send to Junk, then the first rule that matches acts. A journal of every action, each with **Undo** |
| Coucou → Mail → Rules, Banned words, VIP | "If the sender / domain / subject / text / account contains … then Junk / important / read / move to a folder". Banned words are whole words: "sale" does not catch "Salesforce". A VIP is an address or a whole domain (`@client.com`) |
| Turn on **Mail** next to the model in the chat | The AI gets your 40 newest messages (text and attachments for the newest and for the ones you name) and may answer with a plan: sort, reply drafts, sends, a mail merge that gives each recipient their page of a PDF, saving attachments. The plan shows as a card: the mails to send with every Cc and attachment (hover a subject for the text), the ones that can't be sent in red, **Send me a test**, and **Run**, which waits 5 seconds (Cancel) before anything happens |

Coucou never deletes a message, and never sends one without your click on Run. Your mail
goes only to the chat's AI you picked (your key or your subscription), and only while the
chat's Mail switch is on. On some company PCs, Outlook may ask whether to let a program use
it: that is Outlook's own security prompt, set by your IT department.
