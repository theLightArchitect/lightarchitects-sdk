<!-- uuid: 9d413b9c-bf64-4752-be3e-630b9b203f0c -->
<!-- source: https://csrc.nist.gov/pubs/sp/800/218/final | version: SP 800-218 v1.1 | scraped: 2026-05-04 | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->
<!-- gate: [S] -->

**You are viewing this page in an unauthorized frame window.**

This is a potential security issue, you are being redirected to [https://csrc.nist.gov](https://csrc.nist.gov/).

![](https://csrc.nist.gov/dist/uswds/img/us_flag_small.png)

An official website of the United States government


Here’s how you know

Here’s how you know

![](https://csrc.nist.gov/dist/uswds/img/icon-dot-gov.svg)

**Official websites use .gov**

A
**.gov** website belongs to an official government
organization in the United States.


![](https://csrc.nist.gov/dist/uswds/img/icon-https.svg)

**Secure .gov websites use HTTPS**

A
**lock** (
LockLocked padlock icon) or **https://** means you’ve safely connected to
the .gov website. Share sensitive information only on official,
secure websites.


[![National Institute of Standards and Technology](https://csrc.nist.gov/CSRC/media/images/svg/nist-logo.svg)](https://www.nist.gov/)

SearchSearch

[CSRC MENU](https://csrc.nist.gov/pubs/sp/800/218/final#)

SearchSearch

- [Projects](https://csrc.nist.gov/projects)
- [Publications\\
Expand or Collapse](https://csrc.nist.gov/publications)






[Drafts for Public Comment](https://csrc.nist.gov/publications/drafts-open-for-comment)



[All Public Drafts](https://csrc.nist.gov/publications/draft-pubs)



[Final Pubs](https://csrc.nist.gov/publications/final-pubs)



[FIPS (standards)](https://csrc.nist.gov/publications/fips)







[Special Publications (SPs)](https://csrc.nist.gov/publications/sp)



[IR (interagency/internal reports)](https://csrc.nist.gov/publications/ir)



[CSWP (cybersecurity white papers)](https://csrc.nist.gov/publications/cswp)



[ITL Bulletins](https://csrc.nist.gov/publications/itl-bulletin)







[Project Descriptions](https://csrc.nist.gov/publications/project-description)



[Journal Articles](https://csrc.nist.gov/publications/journal-article)



[Conference Papers](https://csrc.nist.gov/publications/conference-paper)



[Books](https://csrc.nist.gov/publications/book)

- [Topics\\
Expand or Collapse](https://csrc.nist.gov/topics)






[Security & Privacy](https://csrc.nist.gov/Topics/Security-and-Privacy)



[Applications](https://csrc.nist.gov/Topics/Applications)







[Technologies](https://csrc.nist.gov/Topics/Technologies)



[Sectors](https://csrc.nist.gov/Topics/Sectors)







[Laws & Regulations](https://csrc.nist.gov/Topics/Laws-and-Regulations)



[Activities & Products](https://csrc.nist.gov/Topics/Activities-and-Products)

- [News & Updates](https://csrc.nist.gov/news)
- [Events](https://csrc.nist.gov/events)
- [Glossary](https://csrc.nist.gov/glossary)
- [About CSRC\\
Expand or Collapse](https://csrc.nist.gov/about)






**[Computer Security Division](https://csrc.nist.gov/Groups/Computer-Security-Division)**





  - [Cryptographic Technology](https://csrc.nist.gov/Groups/Computer-Security-Division/Cryptographic-Technology)
  - [Secure Systems and Applications](https://csrc.nist.gov/Groups/Computer-Security-Division/Secure-Systems-and-Applications)
  - [Security Components and Mechanisms](https://csrc.nist.gov/Groups/Computer-Security-Division/Security-Components-and-Mechanisms)
  - [Security Engineering and Risk Management](https://csrc.nist.gov/Groups/Computer-Security-Division/Security-Engineering-and-Risk-Management)
  - [Security Testing, Validation, and Measurement](https://csrc.nist.gov/Groups/Computer-Security-Division/Security-Testing-Validation-and-Measurement)

**[Applied Cybersecurity Division](https://csrc.nist.gov/Groups/Applied-Cybersecurity-Division)**

  - [Cybersecurity and Privacy Applications](https://csrc.nist.gov/Groups/Applied-Cybersecurity-Division/Cybersecurity-and-Privacy-Applications)
  - [National Cybersecurity Center of Excellence (NCCoE)](https://csrc.nist.gov/Groups/Applied-Cybersecurity-Division/National-Cybersecurity-Center-of-Excellence)
  - [National Initiative for Cybersecurity Education (NICE)](https://www.nist.gov/nice/)

[Contact Us](https://csrc.nist.gov/contact)

[Information Technology Laboratory](https://www.nist.gov/itl)

[Computer Security Resource Center](https://csrc.nist.gov/)

[![CSRC Logo](https://csrc.nist.gov/CSRC/Media/images/nist-logo-csrc-white.svg)](https://csrc.nist.gov/)

[![CSRC Logo](https://csrc.nist.gov/CSRC/Media/images/nist-logo-csrc-white.svg)](https://csrc.nist.gov/)

[Publications](https://csrc.nist.gov/publications)

### NIST SP 800-218

# Secure Software Development Framework (SSDF) Version 1.1: Recommendations for Mitigating the Risk of Software Vulnerabilities

[Share to Facebook](https://www.facebook.com/sharer/sharer.php?u=https%3A%2F%2Fcsrc.nist.gov%2Fpubs%2Fsp%2F800%2F218%2Ffinal) [Share to X](https://x.com/share?url=https%3A%2F%2Fcsrc.nist.gov%2Fpubs%2Fsp%2F800%2F218%2Ffinal) [Share to LinkedIn](https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Fcsrc.nist.gov%2Fpubs%2Fsp%2F800%2F218%2Ffinal&source=csrc.nist.gov) [Share ia Email](mailto:?subject=csrc.nist.gov&body=Check%20out%20this%20site%20https://csrc.nist.gov/pubs/sp/800/218/final)

[Documentation](https://csrc.nist.gov/pubs/sp/800/218/final#pubs-documentation) [Topics](https://csrc.nist.gov/pubs/sp/800/218/final#pubs-topics)

**Date Published:** February 2022

**Supersedes:**[CSWP 13 (04/23/2020)](https://csrc.nist.gov/pubs/cswp/13/mitigating-risk-of-software-vulnerabilities-ssdf/final)

#### Author(s)

Murugiah Souppaya (NIST), Karen Scarfone (Scarfone Cybersecurity), Donna Dodson

#### Abstract

Few software development life cycle (SDLC) models explicitly address software security in detail, so secure software development practices usually need to be added to each SDLC model to ensure that the software being developed is well-secured. This document recommends the Secure Software Development Framework (SSDF) – a core set of high-level secure software development practices that can be integrated into each SDLC implementation. Following these practices should help software producers reduce the number of vulnerabilities in released software, mitigate the potential impact of the exploitation of undetected or unaddressed vulnerabilities, and address the root causes of vulnerabilities to prevent future recurrences. Because the framework provides a common vocabulary for secure software development, software purchasers and consumers can also use it to foster communications with suppliers in acquisition processes and other management activities.

Few software development life cycle (SDLC) models explicitly address software security in detail, so secure software development practices usually need to be added to each SDLC model to ensure that the software being developed is well-secured. This document recommends the Secure Software Development...
[See full abstract](https://csrc.nist.gov/pubs/sp/800/218/final#pubs-abstract-header)

Few software development life cycle (SDLC) models explicitly address software security in detail, so secure software development practices usually need to be added to each SDLC model to ensure that the software being developed is well-secured. This document recommends the Secure Software Development Framework (SSDF) – a core set of high-level secure software development practices that can be integrated into each SDLC implementation. Following these practices should help software producers reduce the number of vulnerabilities in released software, mitigate the potential impact of the exploitation of undetected or unaddressed vulnerabilities, and address the root causes of vulnerabilities to prevent future recurrences. Because the framework provides a common vocabulary for secure software development, software purchasers and consumers can also use it to foster communications with suppliers in acquisition processes and other management activities.

[Hide full abstract](https://csrc.nist.gov/pubs/sp/800/218/final#pubs-abstract-header)

#### Keywords

secure software development; Secure Software Development Framework (SSDF); secure software development practices; software acquisition; software development; software development life cycle (SDLC); software security

##### Control Families

None selected

#### Documentation

**Publication:**

[https://doi.org/10.6028/NIST.SP.800-218](https://doi.org/10.6028/NIST.SP.800-218)

[Download URL](https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-218.pdf)

[Potential updates (xlsx)](https://csrc.nist.gov/files/pubs/sp/800/218/final/docs/sp800-218-potential-updates.xlsx)

[Translations](https://www.nist.gov/cybersecurity/translations#ssdf)

**Supplemental Material:**

[SP 800-218 Table in Excel (xlsx)](https://csrc.nist.gov/files/pubs/sp/800/218/final/docs/nist.sp.800-218.ssdf-table.xlsx)

[Delta from April 2020 paper (docx)](https://csrc.nist.gov/files/pubs/sp/800/218/final/docs/800-218-deltas-from-wp-to-final.docx)

[Delta from September 2021 public draft (docx)](https://csrc.nist.gov/files/pubs/sp/800/218/final/docs/800-218-deltas-from-draft-to-final.docx)

[SSDF Project homepage](https://csrc.nist.gov/projects/ssdf)

[Executive Order 14028, Improving the Nation's Cybersecurity](https://www.nist.gov/itl/executive-order-improving-nations-cybersecurity)

**Publication Parts:**

[SP 800-218A](https://csrc.nist.gov/pubs/sp/800/218/a/final)

**Related NIST Publications:**

[Other](https://csrc.nist.gov/pubs/other/2022/02/04/software-supply-chain-security-guidance-eo-14028-s/final)

**Document History:**

09/30/21: [SP 800-218 (Draft)](https://csrc.nist.gov/pubs/sp/800/218/ipd)

02/03/22: SP 800-218 (Final)

#### Topics

**Security and Privacy**

[cybersecurity supply chain risk management](https://csrc.nist.gov/topics/security-and-privacy/cyber-supply-chain-risk-management), [vulnerability management](https://csrc.nist.gov/topics/security-and-privacy/risk-management/vulnerabilities/vulnerability-management)

**Technologies**

[software & firmware](https://csrc.nist.gov/topics/technologies/software-firmware)

**Laws and Regulations**

[Executive Order 14028](https://csrc.nist.gov/topics/laws-and-regulations/executive-documents/executive-order-14028)

[![National Institute of Standards and Technology logo](https://csrc.nist.gov/CSRC/Media/images/nist-logo-brand-white.svg)](https://www.nist.gov/ "National Institute of Standards and Technology")

**HEADQUARTERS**

100 Bureau Drive

Gaithersburg, MD 20899


- [_X_ (link is external)](https://x.com/NISTCyber)
- [_facebook_ (link is external)](https://www.facebook.com/NIST)
- [_linkedin_ (link is external)](https://www.linkedin.com/company/nist)
- [_instagram_ (link is external)](https://www.instagram.com/usnistgov/)
- [_youtube_ (link is external)](https://www.youtube.com/user/USNISTGOV)
- [_rss_](https://www.nist.gov/news-events/nist-rss-feeds)
- [_govdelivery_ (link is external)](https://public.govdelivery.com/accounts/USNIST/subscriber/new?qsp=USNIST_3 "Subscribe to CSRC and publication updates, and other NIST cybersecurity news")

Want updates about CSRC and our publications?
[Subscribe](https://public.govdelivery.com/accounts/USNIST/subscriber/new?qsp=USNIST_3)

[![National Institute of Standards and Technology logo](https://csrc.nist.gov/CSRC/Media/images/logo_rev.png)](https://www.nist.gov/ "National Institute of Standards and Technology")

[Contact Us](https://csrc.nist.gov/about/contact) \|
[Our Other Offices](https://www.nist.gov/about-nist/visit)

Send inquiries to [csrc-inquiry@nist.gov](mailto:csrc-inquiry@nist.gov?subject=CSRC%20Inquiry)

- [Site Privacy](https://www.nist.gov/privacy-policy)
- [Accessibility](https://www.nist.gov/oism/accessibility)
- [Privacy Program](https://www.nist.gov/privacy)
- [Copyrights](https://www.nist.gov/oism/copyrights)
- [Vulnerability Disclosure](https://www.commerce.gov/vulnerability-disclosure-policy)
- [No Fear Act Policy](https://www.nist.gov/no-fear-act-policy)
- [FOIA](https://www.nist.gov/foia)
- [Environmental Policy](https://www.nist.gov/environmental-policy-statement)
- [Scientific Integrity](https://www.nist.gov/summary-report-scientific-integrity)
- [Information Quality Standards](https://www.nist.gov/nist-information-quality-standards)
- [Commerce.gov](https://www.commerce.gov/)
- [Science.gov](https://www.science.gov/)
- [USA.gov](https://www.usa.gov/)
- [Vote.gov](https://vote.gov/)