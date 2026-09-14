# OAuth release checklist

Development, staging and production must use separate OAuth projects and registrations. Release
builds read public client IDs from repository variables and compile them into the desktop binary:

- `GOOGLE_OAUTH_CLIENT_ID`
- `MICROSOFT_OAUTH_CLIENT_ID`

They are public identifiers, not client secrets. Threadbox is a native public client and uses the
system browser, PKCE, state validation and a random loopback callback port.

## Google production review

The production Google Cloud project must enable Calendar API and Gmail API. Configure an external
OAuth consent screen and a Desktop app client. Request only the capabilities implemented in the
interface and selected by the user.

Before submission:

1. publish a product homepage, privacy policy and terms on a verified domain;
2. describe local storage, optional cloud transcription and optional cloud meeting analysis;
3. state that Google data is used only for the visible calendar, availability and mail features;
4. show separate read, compose and send controls in the verification video;
5. demonstrate connection, revocation, disconnection and retained local project history;
6. justify every Calendar and Gmail scope and explain why a narrower scope cannot provide that
   specific visible feature;
7. declare whether restricted Google data is ever transmitted to a cloud language model and show
   the explicit per-operation consent if it is;
8. complete any restricted-scope security assessment requested by Google;
9. add the approved production Desktop client ID as `GOOGLE_OAUTH_CLIENT_ID`.

Do not publish a build that requests an unapproved sensitive or restricted scope. Google can impose
an unverified warning and a lifetime new-user cap on such a project.

## Microsoft production registration

Create a production app registration in Microsoft Entra ID with supported account types matching
the intended audience. Add the Mobile and desktop applications platform, enable public client flows
and register `http://localhost` as the loopback redirect. Ephemeral localhost ports are expected for
native applications.

Configure only delegated Microsoft Graph permissions represented by Threadbox controls, grant no
application permissions and add the application client ID as `MICROSOFT_OAUTH_CLIENT_ID`.

## Release evidence

Attach these items to the release record:

- Google verification state and approved scope list;
- Microsoft registration ID and delegated permission list;
- screenshots of both consent screens;
- clean-account connect, revoke, repair and disconnect results;
- confirmation that no OAuth client secret is present in the binary or repository.

Primary references:

- [Google OAuth policies](https://developers.google.com/identity/protocols/oauth2/policies)
- [Google verification requirements](https://support.google.com/cloud/answer/13464321)
- [Google restricted scope verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)
- [Microsoft redirect URI guidance](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
