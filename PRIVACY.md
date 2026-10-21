# Privacy policy

Garmin Companion. Last change: 2026-10-21.

Garmin Companion is a free, open source app for Android, iOS, Windows, macOS
and Linux. Omar Zunic makes it. Garmin Ltd. does not make or support it.

The app keeps your data on your device. It sends data to a server only for the
cloud coach, and only when you use the cloud coach.

## Data on your device

- The app signs in to Garmin Connect in a Garmin web page. Garmin gets your
  Garmin email and password. The app does not get your password.
- The app keeps the Garmin sign-in tokens on your device, in the secure storage
  of the system.
- The app downloads your Garmin data (activities, routes, sleep, heart rate,
  weight, body data and other health data) from Garmin to your device. It keeps
  this data in a local database on the device.
- The app does not send this data to the developer. It has no analytics, no
  advertising and no crash reports.

## The cloud coach

The coach answers questions about your data. You choose where it runs:

- **Cloud** (the default): the app sends your question and the data rows that
  the question needs to the coach server of Garmin Companion. The server sends
  them to OpenRouter, and OpenRouter sends them to an AI model provider. The
  server does not store or log your questions, the answers or your data.
  OpenRouter and the model provider process the data under their own policies.
- **Your own OpenRouter key**: the app sends the question and the data rows
  directly to OpenRouter. The coach server gets nothing.
- **Ollama**: the app sends the question and the data rows to the Ollama
  computer that you choose. Nobody else gets them.
- **Claude Code** (desktop only): the app starts the Claude Code command on
  your computer. It sends your question and the Garmin data that the tools of
  the app return to this command. The command sends them to its AI model
  provider, under your own account and the policies of that provider. The coach
  server gets nothing.

Chat history stays on your device.

## Data on the coach server

The coach server keeps only this data:

- **An install id.** This is a random number that the app gets when it first
  uses the cloud coach. The server uses it to count the free credits. It has no
  link to your name, your Garmin account or your device.
- **Credit counters** for each install id and each account.
- **Short-term request counts for each network address.** These stop abuse.
  The server does not store the address with your install id or your account.
- **Your email address**, only if you sign in for premium. The server uses it
  to send a sign-in code and to connect your purchase to your account. The
  email service Resend sends the code.
- **Your plan and the purchase record**, only if you buy premium. Polar (on
  desktop), Apple or Google take the payment. The server does not get your card
  details.

The server uses HTTPS for all traffic.

## Sharing

The developer does not sell your data. The developer does not share your data
with anyone except the services above, and only to make the app work.

## Delete your data

- **Data on your device:** uninstall the app, or clear the app data in the
  system settings.
- **Data on the coach server:** send an email to mail@omarzunic.com from the
  email address of your account, with the subject "Delete my data". The
  developer deletes your account, your sessions, your plan record and your
  email address in 30 days or less. Install ids and counters expire by
  themselves in 400 days or less.

## Children

The app is not for children under 13.

## Changes

A new version of this policy replaces the old version in this file. The
history of this file in the repository shows each change.

## Contact

mail@omarzunic.com
