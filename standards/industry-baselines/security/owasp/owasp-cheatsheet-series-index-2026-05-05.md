[Skip to content](https://cheatsheetseries.owasp.org/IndexTopTen.html#owasp-top-ten-2021-related-cheat-sheets)

[![logo](https://cheatsheetseries.owasp.org/assets/OWASP_Logo.svg)](https://cheatsheetseries.owasp.org/index.html "OWASP Cheat Sheet Series")

OWASP Cheat Sheet Series



Index Top 10



Initializing search


[OWASP/CheatSheetSeries\\
\\
\\
- 31.9k\\
- 4.4k](https://github.com/OWASP/CheatSheetSeries "Go to repository")

[![logo](https://cheatsheetseries.owasp.org/assets/OWASP_Logo.svg)](https://cheatsheetseries.owasp.org/index.html "OWASP Cheat Sheet Series")
OWASP Cheat Sheet Series


[OWASP/CheatSheetSeries\\
\\
\\
- 31.9k\\
- 4.4k](https://github.com/OWASP/CheatSheetSeries "Go to repository")

- [Introduction](https://cheatsheetseries.owasp.org/index.html)
- [Index Alphabetical](https://cheatsheetseries.owasp.org/Glossary.html)
- [Index ASVS](https://cheatsheetseries.owasp.org/IndexASVS.html)
- [Index MASVS](https://cheatsheetseries.owasp.org/IndexMASVS.html)
- [Index Proactive Controls](https://cheatsheetseries.owasp.org/IndexProactiveControls.html)
- [ ]
Index Top 10
[Index Top 10](https://cheatsheetseries.owasp.org/IndexTopTen.html)
Table of contents


  - [A01:2021 – Broken Access Control](https://cheatsheetseries.owasp.org/IndexTopTen.html#a012021-broken-access-control)
  - [A02:2021 – Cryptographic Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a022021-cryptographic-failures)
  - [A03:2021 – Injection](https://cheatsheetseries.owasp.org/IndexTopTen.html#a032021-injection)
  - [A04:2021 – Insecure Design](https://cheatsheetseries.owasp.org/IndexTopTen.html#a042021-insecure-design)
  - [A05:2021 – Security Misconfiguration](https://cheatsheetseries.owasp.org/IndexTopTen.html#a052021-security-misconfiguration)
  - [A06:2021 – Vulnerable and Outdated Components](https://cheatsheetseries.owasp.org/IndexTopTen.html#a062021-vulnerable-and-outdated-components)
  - [A07:2021 – Identification and Authentication Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a072021-identification-and-authentication-failures)
  - [A08:2021 – Software and Data Integrity Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a082021-software-and-data-integrity-failures)
  - [A09:2021 – Security Logging and Monitoring Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a092021-security-logging-and-monitoring-failures)
  - [A10:2021 – Server-Side Request Forgery (SSRF)](https://cheatsheetseries.owasp.org/IndexTopTen.html#a102021-server-side-request-forgery-ssrf)
  - [A11:2021 – Next Steps](https://cheatsheetseries.owasp.org/IndexTopTen.html#a112021-next-steps)

- [ ]
Cheatsheets

Cheatsheets


  - [AI Agent Security](https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html)
  - [AJAX Security](https://cheatsheetseries.owasp.org/cheatsheets/AJAX_Security_Cheat_Sheet.html)
  - [Abuse Case](https://cheatsheetseries.owasp.org/cheatsheets/Abuse_Case_Cheat_Sheet.html)
  - [Access Control](https://cheatsheetseries.owasp.org/cheatsheets/Access_Control_Cheat_Sheet.html)
  - [Attack Surface Analysis](https://cheatsheetseries.owasp.org/cheatsheets/Attack_Surface_Analysis_Cheat_Sheet.html)
  - [Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
  - [Authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
  - [Authorization Testing Automation](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Testing_Automation_Cheat_Sheet.html)
  - [Automotive Security](https://cheatsheetseries.owasp.org/cheatsheets/Automotive_Security_Cheat_Sheet.html)
  - [Bean Validation](https://cheatsheetseries.owasp.org/cheatsheets/Bean_Validation_Cheat_Sheet.html)
  - [Browser Extension Vulnerabilities](https://cheatsheetseries.owasp.org/cheatsheets/Browser_Extension_Vulnerabilities_Cheat_Sheet.html)
  - [Business Logic Security](https://cheatsheetseries.owasp.org/cheatsheets/Business_Logic_Security_Cheat_Sheet.html)
  - [C-Based Toolchain Hardening](https://cheatsheetseries.owasp.org/cheatsheets/C-Based_Toolchain_Hardening_Cheat_Sheet.html)
  - [CI CD Security](https://cheatsheetseries.owasp.org/cheatsheets/CI_CD_Security_Cheat_Sheet.html)
  - [Choosing and Using Security Questions](https://cheatsheetseries.owasp.org/cheatsheets/Choosing_and_Using_Security_Questions_Cheat_Sheet.html)
  - [Clickjacking Defense](https://cheatsheetseries.owasp.org/cheatsheets/Clickjacking_Defense_Cheat_Sheet.html)
  - [Content Security Policy](https://cheatsheetseries.owasp.org/cheatsheets/Content_Security_Policy_Cheat_Sheet.html)
  - [Cookie Theft Mitigation](https://cheatsheetseries.owasp.org/cheatsheets/Cookie_Theft_Mitigation_Cheat_Sheet.html)
  - [Credential Stuffing Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Credential_Stuffing_Prevention_Cheat_Sheet.html)
  - [Cross-Site Request Forgery Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
  - [Cross Site Scripting Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
  - [Cryptographic Storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)
  - [DOM Clobbering Prevention](https://cheatsheetseries.owasp.org/cheatsheets/DOM_Clobbering_Prevention_Cheat_Sheet.html)
  - [DOM based XSS Prevention](https://cheatsheetseries.owasp.org/cheatsheets/DOM_based_XSS_Prevention_Cheat_Sheet.html)
  - [Database Security](https://cheatsheetseries.owasp.org/cheatsheets/Database_Security_Cheat_Sheet.html)
  - [Denial of Service](https://cheatsheetseries.owasp.org/cheatsheets/Denial_of_Service_Cheat_Sheet.html)
  - [Dependency Graph SBOM](https://cheatsheetseries.owasp.org/cheatsheets/Dependency_Graph_SBOM_Cheat_Sheet.html)
  - [Deserialization](https://cheatsheetseries.owasp.org/cheatsheets/Deserialization_Cheat_Sheet.html)
  - [Django REST Framework](https://cheatsheetseries.owasp.org/cheatsheets/Django_REST_Framework_Cheat_Sheet.html)
  - [Django Security](https://cheatsheetseries.owasp.org/cheatsheets/Django_Security_Cheat_Sheet.html)
  - [Docker Security](https://cheatsheetseries.owasp.org/cheatsheets/Docker_Security_Cheat_Sheet.html)
  - [DotNet Security](https://cheatsheetseries.owasp.org/cheatsheets/DotNet_Security_Cheat_Sheet.html)
  - [Drone Security](https://cheatsheetseries.owasp.org/cheatsheets/Drone_Security_Cheat_Sheet.html)
  - [Email Validation and Verification](https://cheatsheetseries.owasp.org/cheatsheets/Email_Validation_and_Verification_Cheat_Sheet.html)
  - [Error Handling](https://cheatsheetseries.owasp.org/cheatsheets/Error_Handling_Cheat_Sheet.html)
  - [File Upload](https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html)
  - [Forgot Password](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
  - [GitHub Actions Security](https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html)
  - [GraphQL](https://cheatsheetseries.owasp.org/cheatsheets/GraphQL_Cheat_Sheet.html)
  - [HTML5 Security](https://cheatsheetseries.owasp.org/cheatsheets/HTML5_Security_Cheat_Sheet.html)
  - [HTTP Headers](https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Headers_Cheat_Sheet.html)
  - [HTTP Strict Transport Security](https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Strict_Transport_Security_Cheat_Sheet.html)
  - [Infrastructure as Code Security](https://cheatsheetseries.owasp.org/cheatsheets/Infrastructure_as_Code_Security_Cheat_Sheet.html)
  - [Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Injection_Prevention_Cheat_Sheet.html)
  - [Injection Prevention in Java](https://cheatsheetseries.owasp.org/cheatsheets/Injection_Prevention_in_Java_Cheat_Sheet.html)
  - [Input Validation](https://cheatsheetseries.owasp.org/cheatsheets/Input_Validation_Cheat_Sheet.html)
  - [Insecure Direct Object Reference Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Insecure_Direct_Object_Reference_Prevention_Cheat_Sheet.html)
  - [JAAS](https://cheatsheetseries.owasp.org/cheatsheets/JAAS_Cheat_Sheet.html)
  - [JSON Web Token for Java](https://cheatsheetseries.owasp.org/cheatsheets/JSON_Web_Token_for_Java_Cheat_Sheet.html)
  - [Java Security](https://cheatsheetseries.owasp.org/cheatsheets/Java_Security_Cheat_Sheet.html)
  - [Key Management](https://cheatsheetseries.owasp.org/cheatsheets/Key_Management_Cheat_Sheet.html)
  - [Kubernetes Security](https://cheatsheetseries.owasp.org/cheatsheets/Kubernetes_Security_Cheat_Sheet.html)
  - [LDAP Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/LDAP_Injection_Prevention_Cheat_Sheet.html)
  - [LLM Prompt Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)
  - [Laravel](https://cheatsheetseries.owasp.org/cheatsheets/Laravel_Cheat_Sheet.html)
  - [Legacy Application Management](https://cheatsheetseries.owasp.org/cheatsheets/Legacy_Application_Management_Cheat_Sheet.html)
  - [Logging](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html)
  - [Logging Vocabulary](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Vocabulary_Cheat_Sheet.html)
  - [MCP Security](https://cheatsheetseries.owasp.org/cheatsheets/MCP_Security_Cheat_Sheet.html)
  - [Mass Assignment](https://cheatsheetseries.owasp.org/cheatsheets/Mass_Assignment_Cheat_Sheet.html)
  - [Microservices Security](https://cheatsheetseries.owasp.org/cheatsheets/Microservices_Security_Cheat_Sheet.html)
  - [Microservices based Security Arch Doc](https://cheatsheetseries.owasp.org/cheatsheets/Microservices_based_Security_Arch_Doc_Cheat_Sheet.html)
  - [Mobile Application Security](https://cheatsheetseries.owasp.org/cheatsheets/Mobile_Application_Security_Cheat_Sheet.html)
  - [Multi Tenant Security](https://cheatsheetseries.owasp.org/cheatsheets/Multi_Tenant_Security_Cheat_Sheet.html)
  - [Multifactor Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html)
  - [NPM Security](https://cheatsheetseries.owasp.org/cheatsheets/NPM_Security_Cheat_Sheet.html)
  - [Network Segmentation](https://cheatsheetseries.owasp.org/cheatsheets/Network_Segmentation_Cheat_Sheet.html)
  - [NoSQL Security](https://cheatsheetseries.owasp.org/cheatsheets/NoSQL_Security_Cheat_Sheet.html)
  - [NodeJS Docker](https://cheatsheetseries.owasp.org/cheatsheets/NodeJS_Docker_Cheat_Sheet.html)
  - [Nodejs Security](https://cheatsheetseries.owasp.org/cheatsheets/Nodejs_Security_Cheat_Sheet.html)
  - [OAuth2](https://cheatsheetseries.owasp.org/cheatsheets/OAuth2_Cheat_Sheet.html)
  - [OS Command Injection Defense](https://cheatsheetseries.owasp.org/cheatsheets/OS_Command_Injection_Defense_Cheat_Sheet.html)
  - [PHP Configuration](https://cheatsheetseries.owasp.org/cheatsheets/PHP_Configuration_Cheat_Sheet.html)
  - [Password Storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
  - [Pinning](https://cheatsheetseries.owasp.org/cheatsheets/Pinning_Cheat_Sheet.html)
  - [Prototype Pollution Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Prototype_Pollution_Prevention_Cheat_Sheet.html)
  - [Query Parameterization](https://cheatsheetseries.owasp.org/cheatsheets/Query_Parameterization_Cheat_Sheet.html)
  - [REST Assessment](https://cheatsheetseries.owasp.org/cheatsheets/REST_Assessment_Cheat_Sheet.html)
  - [REST Security](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html)
  - [Ruby on Rails](https://cheatsheetseries.owasp.org/cheatsheets/Ruby_on_Rails_Cheat_Sheet.html)
  - [SAML Security](https://cheatsheetseries.owasp.org/cheatsheets/SAML_Security_Cheat_Sheet.html)
  - [SQL Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/SQL_Injection_Prevention_Cheat_Sheet.html)
  - [Secrets Management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
  - [Secure AI Model Ops](https://cheatsheetseries.owasp.org/cheatsheets/Secure_AI_Model_Ops_Cheat_Sheet.html)
  - [Secure Cloud Architecture](https://cheatsheetseries.owasp.org/cheatsheets/Secure_Cloud_Architecture_Cheat_Sheet.html)
  - [Secure Code Review](https://cheatsheetseries.owasp.org/cheatsheets/Secure_Code_Review_Cheat_Sheet.html)
  - [Secure Coding with AI](https://cheatsheetseries.owasp.org/cheatsheets/Secure_Coding_with_AI_Cheat_Sheet.html)
  - [Secure Product Design](https://cheatsheetseries.owasp.org/cheatsheets/Secure_Product_Design_Cheat_Sheet.html)
  - [Securing Cascading Style Sheets](https://cheatsheetseries.owasp.org/cheatsheets/Securing_Cascading_Style_Sheets_Cheat_Sheet.html)
  - [Security Terminology](https://cheatsheetseries.owasp.org/cheatsheets/Security_Terminology_Cheat_Sheet.html)
  - [Server Side Request Forgery Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)
  - [Serverless FaaS Security](https://cheatsheetseries.owasp.org/cheatsheets/Serverless_FaaS_Security_Cheat_Sheet.html)
  - [Session Management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
  - [Software Supply Chain Security](https://cheatsheetseries.owasp.org/cheatsheets/Software_Supply_Chain_Security_Cheat_Sheet.html)
  - [Subdomain Takeover Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Subdomain_Takeover_Prevention_Cheat_Sheet.html)
  - [Symfony](https://cheatsheetseries.owasp.org/cheatsheets/Symfony_Cheat_Sheet.html)
  - [TLS Cipher String](https://cheatsheetseries.owasp.org/cheatsheets/TLS_Cipher_String_Cheat_Sheet.html)
  - [Third Party Javascript Management](https://cheatsheetseries.owasp.org/cheatsheets/Third_Party_Javascript_Management_Cheat_Sheet.html)
  - [Third Party Payment Gateway Integration](https://cheatsheetseries.owasp.org/cheatsheets/Third_Party_Payment_Gateway_Integration_Cheat_Sheet.html)
  - [Threat Modeling](https://cheatsheetseries.owasp.org/cheatsheets/Threat_Modeling_Cheat_Sheet.html)
  - [Transaction Authorization](https://cheatsheetseries.owasp.org/cheatsheets/Transaction_Authorization_Cheat_Sheet.html)
  - [Transport Layer Protection](https://cheatsheetseries.owasp.org/cheatsheets/Transport_Layer_Protection_Cheat_Sheet.html)
  - [Transport Layer Security](https://cheatsheetseries.owasp.org/cheatsheets/Transport_Layer_Security_Cheat_Sheet.html)
  - [Unvalidated Redirects and Forwards](https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html)
  - [User Privacy Protection](https://cheatsheetseries.owasp.org/cheatsheets/User_Privacy_Protection_Cheat_Sheet.html)
  - [Virtual Patching](https://cheatsheetseries.owasp.org/cheatsheets/Virtual_Patching_Cheat_Sheet.html)
  - [Vulnerability Disclosure](https://cheatsheetseries.owasp.org/cheatsheets/Vulnerability_Disclosure_Cheat_Sheet.html)
  - [Vulnerable Dependency Management](https://cheatsheetseries.owasp.org/cheatsheets/Vulnerable_Dependency_Management_Cheat_Sheet.html)
  - [WebSocket Security](https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html)
  - [Web Service Security](https://cheatsheetseries.owasp.org/cheatsheets/Web_Service_Security_Cheat_Sheet.html)
  - [XML External Entity Prevention](https://cheatsheetseries.owasp.org/cheatsheets/XML_External_Entity_Prevention_Cheat_Sheet.html)
  - [XML Security](https://cheatsheetseries.owasp.org/cheatsheets/XML_Security_Cheat_Sheet.html)
  - [XSS Filter Evasion](https://cheatsheetseries.owasp.org/cheatsheets/XSS_Filter_Evasion_Cheat_Sheet.html)
  - [XS Leaks](https://cheatsheetseries.owasp.org/cheatsheets/XS_Leaks_Cheat_Sheet.html)
  - [Zero Trust Architecture](https://cheatsheetseries.owasp.org/cheatsheets/Zero_Trust_Architecture_Cheat_Sheet.html)
  - [gRPC Security](https://cheatsheetseries.owasp.org/cheatsheets/gRPC_Security_Cheat_Sheet.html)

Table of contents


- [A01:2021 – Broken Access Control](https://cheatsheetseries.owasp.org/IndexTopTen.html#a012021-broken-access-control)
- [A02:2021 – Cryptographic Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a022021-cryptographic-failures)
- [A03:2021 – Injection](https://cheatsheetseries.owasp.org/IndexTopTen.html#a032021-injection)
- [A04:2021 – Insecure Design](https://cheatsheetseries.owasp.org/IndexTopTen.html#a042021-insecure-design)
- [A05:2021 – Security Misconfiguration](https://cheatsheetseries.owasp.org/IndexTopTen.html#a052021-security-misconfiguration)
- [A06:2021 – Vulnerable and Outdated Components](https://cheatsheetseries.owasp.org/IndexTopTen.html#a062021-vulnerable-and-outdated-components)
- [A07:2021 – Identification and Authentication Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a072021-identification-and-authentication-failures)
- [A08:2021 – Software and Data Integrity Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a082021-software-and-data-integrity-failures)
- [A09:2021 – Security Logging and Monitoring Failures](https://cheatsheetseries.owasp.org/IndexTopTen.html#a092021-security-logging-and-monitoring-failures)
- [A10:2021 – Server-Side Request Forgery (SSRF)](https://cheatsheetseries.owasp.org/IndexTopTen.html#a102021-server-side-request-forgery-ssrf)
- [A11:2021 – Next Steps](https://cheatsheetseries.owasp.org/IndexTopTen.html#a112021-next-steps)

# OWASP Top Ten 2021 : Related Cheat Sheets [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#owasp-top-ten-2021-related-cheat-sheets "Permanent link")

The [OWASP Top Ten](https://owasp.org/www-project-top-ten/) is a standard awareness document for developers and web application security. It represents a broad consensus about the most critical security risks to web applications.

This cheat sheet will help users of the [OWASP Top Ten](https://owasp.org/Top10/) identify which cheat sheets map to each security category. This mapping is based the [OWASP Top Ten 2021 version](https://owasp.org/Top10/#welcome-to-the-owasp-top-10-2021).

## [A01:2021 – Broken Access Control](https://owasp.org/Top10/A01_2021-Broken_Access_Control/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a012021-broken-access-control "Permanent link")

- [Authorization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
- [Insecure Direct Object Reference Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Insecure_Direct_Object_Reference_Prevention_Cheat_Sheet.html)
- [Transaction Authorization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Transaction_Authorization_Cheat_Sheet.html)
- [Cross-Site Request Forgery Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)

## [A02:2021 – Cryptographic Failures](https://owasp.org/Top10/A02_2021-Cryptographic_Failures/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a022021-cryptographic-failures "Permanent link")

- [Cryptographic Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)
- [Transport Layer Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Transport_Layer_Security_Cheat_Sheet.html)
- [HTTP Strict Transport Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Strict_Transport_Security_Cheat_Sheet.html)
- [Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
- [Key Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Key_Management_Cheat_Sheet.html)
- [Pinning Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Pinning_Cheat_Sheet.html)

## [A03:2021 – Injection](https://owasp.org/Top10/A03_2021-Injection/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a032021-injection "Permanent link")

- [Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Injection_Prevention_Cheat_Sheet.html)
- [LDAP Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LDAP_Injection_Prevention_Cheat_Sheet.html)
- [OS Command Injection Defense Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/OS_Command_Injection_Defense_Cheat_Sheet.html)
- [Injection Prevention in Java Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Injection_Prevention_in_Java_Cheat_Sheet.html)
- [SQL Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/SQL_Injection_Prevention_Cheat_Sheet.html)
- [Query Parameterization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Query_Parameterization_Cheat_Sheet.html)
- [Cross Site Scripting Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
- [DOM based XSS Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/DOM_based_XSS_Prevention_Cheat_Sheet.html)
- [XSS Filter Evasion Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/XSS_Filter_Evasion_Cheat_Sheet.html)
- [Content Security Policy Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Content_Security_Policy_Cheat_Sheet.html)

## [A04:2021 – Insecure Design](https://owasp.org/Top10/A04_2021-Insecure_Design/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a042021-insecure-design "Permanent link")

- [Threat Modeling Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Threat_Modeling_Cheat_Sheet.html)
- [Abuse Case Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Abuse_Case_Cheat_Sheet.html)
- [Attack Surface Analysis Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Attack_Surface_Analysis_Cheat_Sheet.html)

## [A05:2021 – Security Misconfiguration](https://owasp.org/Top10/A05_2021-Security_Misconfiguration/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a052021-security-misconfiguration "Permanent link")

- [Infrastructure as Code Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Infrastructure_as_Code_Security_Cheat_Sheet.html)
- [XML External Entity Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/XML_External_Entity_Prevention_Cheat_Sheet.html)
- [PHP Configuration Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/PHP_Configuration_Cheat_Sheet.html)
- [Docker Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Docker_Security_Cheat_Sheet.html)

## [A06:2021 – Vulnerable and Outdated Components](https://owasp.org/Top10/A06_2021-Vulnerable_and_Outdated_Components/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a062021-vulnerable-and-outdated-components "Permanent link")

- [Vulnerable Dependency Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Vulnerable_Dependency_Management_Cheat_Sheet.html)
- [Third Party JavaScript Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Third_Party_Javascript_Management_Cheat_Sheet.html)
- [npm Security best practices](https://cheatsheetseries.owasp.org/cheatsheets/NPM_Security_Cheat_Sheet.html)

## [A07:2021 – Identification and Authentication Failures](https://owasp.org/Top10/A07_2021-Identification_and_Authentication_Failures/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a072021-identification-and-authentication-failures "Permanent link")

- [Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [Forgot Password Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
- [Choosing and Using Security Questions Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Choosing_and_Using_Security_Questions_Cheat_Sheet.html)
- [Credential Stuffing Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Credential_Stuffing_Prevention_Cheat_Sheet.html)
- [Denial of Service Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Denial_of_Service_Cheat_Sheet.html)
- [JSON Web Token for Java Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/JSON_Web_Token_for_Java_Cheat_Sheet.html)
- [Multifactor Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html)
- [Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [SAML Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/SAML_Security_Cheat_Sheet.html)

## [A08:2021 – Software and Data Integrity Failures](https://owasp.org/Top10/A08_2021-Software_and_Data_Integrity_Failures/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a082021-software-and-data-integrity-failures "Permanent link")

- [Deserialization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Deserialization_Cheat_Sheet.html)

## [A09:2021 – Security Logging and Monitoring Failures](https://owasp.org/Top10/A09_2021-Security_Logging_and_Monitoring_Failures/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a092021-security-logging-and-monitoring-failures "Permanent link")

- [Logging Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html)
- [Application Logging Vocabulary Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Vocabulary_Cheat_Sheet.html)

## [A10:2021 – Server-Side Request Forgery (SSRF)](https://owasp.org/Top10/A10_2021-Server-Side_Request_Forgery_%28SSRF%29/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a102021-server-side-request-forgery-ssrf "Permanent link")

- [Server Side Request Forgery Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)

## [A11:2021 – Next Steps](https://owasp.org/Top10/A11_2021-Next_Steps/) [¶](https://cheatsheetseries.owasp.org/IndexTopTen.html\#a112021-next-steps "Permanent link")

- [Denial of Service Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Denial_of_Service_Cheat_Sheet.html)

©Copyright 2026 - Cheat Sheets Series Team - This work is licensed under [Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/).




Made with
[Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)