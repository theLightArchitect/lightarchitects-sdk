[Jump to Content](https://saif.google/secure-ai-framework/risks#page-content)

[SAIF](https://saif.google/ "Google")

[SAIF Map Tour](https://saif.google/secure-ai-framework/saif-map)

[SAIF](https://saif.google/ "")

- [Secure AI Framework](https://saif.google/secure-ai-framework)
  - [SAIF Map](https://saif.google/secure-ai-framework/saif-map)
  - [Components](https://saif.google/secure-ai-framework/components)
  - [Risks](https://saif.google/secure-ai-framework/risks)
  - [Controls](https://saif.google/secure-ai-framework/controls)
- [Focus on Agents](https://saif.google/focus-on-agents)
- [Why SAIF](https://saif.google/why-saif)
- [AI Development Primer](https://saif.google/ai-development-primer)
- [Risk Self Assessment](https://saif.google/risk-self-assessment)
- [Resources](https://saif.google/resources)

[SAIF Map Tour](https://saif.google/secure-ai-framework/saif-map)

**SAIF 2.0:** [**Secure Agents**](https://saif.google/focus-on-agents) **— Building powerful agents users can trust**

## Risks

* * *

- [Data Poisoning](https://saif.google/secure-ai-framework/risks#data-poisoning)
- [Unauthorized Training Data](https://saif.google/secure-ai-framework/risks#unauthorized-training-data)
- [Model Source Tampering](https://saif.google/secure-ai-framework/risks#model-source-tampering)
- [Excessive Data Handling](https://saif.google/secure-ai-framework/risks#excessive-data-handling)
- [Model Exfiltration](https://saif.google/secure-ai-framework/risks#model-exfiltration)
- [Model Deployment Tampering](https://saif.google/secure-ai-framework/risks#model-deployment-tampering)
- [Denial of ML Service](https://saif.google/secure-ai-framework/risks#denial-of-ml-service)
- [Model Reverse Engineering](https://saif.google/secure-ai-framework/risks#model-reverse-engineering)
- [Insecure Integrated Component](https://saif.google/secure-ai-framework/risks#insecure-integrated-component)
- [Prompt Injection](https://saif.google/secure-ai-framework/risks#prompt-injection)
- [Model Evasion](https://saif.google/secure-ai-framework/risks#model-evasion)
- [Sensitive Data Disclosure (Updated)](https://saif.google/secure-ai-framework/risks#sensitive-data-disclosure)
- [Inferred Sensitive Data](https://saif.google/secure-ai-framework/risks#inferred-sensitive-data)
- [Insecure Model Output](https://saif.google/secure-ai-framework/risks#insecure-model-output)
- [Rogue Actions (Updated)](https://saif.google/secure-ai-framework/risks#rogue-actions)

![](https://www.gstatic.com/marketing-cms/assets/images/f7/dc/f2b86eb74952b21b6af4b6a987d6/header.svg)

## Risks

The following section describes each risk in the SAIF Map, including causes, impact, potential mitigations, and examples of real-world exploitation.

Each risk is mapped to the relevant controls that can be enacted, and is associated with the Model Creator, the Model Consumer, or both, based on who is responsible for enacting the controls that can mitigate the risk:

- Model Creator: Those who train or develop AI models for use by themselves or others.
- Model Consumer: Those who use AI models to build AI-powered products and applications.

This mapping does not specify controls related to Assurance and Governance functions, since Assurance and Governance controls should be applied to _all_ risks, by all parties, across the AI development lifecycle.

For a complete list of controls, see the Controls descriptions.

- [Data Poisoning](https://saif.google/secure-ai-framework/risks#data-poisoning)
- [Unauthorized Training Data](https://saif.google/secure-ai-framework/risks#unauthorized-training-data)
- [Model Source Tampering](https://saif.google/secure-ai-framework/risks#model-source-tampering)
- [Excessive Data Handling](https://saif.google/secure-ai-framework/risks#excessive-data-handling)
- [Model Exfiltration](https://saif.google/secure-ai-framework/risks#model-exfiltration)
- [Model Deployment Tampering](https://saif.google/secure-ai-framework/risks#model-deployment-tampering)
- [Denial of ML Service](https://saif.google/secure-ai-framework/risks#denial-of-ml-service)
- [Model Reverse Engineering](https://saif.google/secure-ai-framework/risks#model-reverse-engineering)
- [Insecure Integrated Component](https://saif.google/secure-ai-framework/risks#insecure-integrated-component)
- [Prompt Injection](https://saif.google/secure-ai-framework/risks#prompt-injection)
- [Model Evasion](https://saif.google/secure-ai-framework/risks#model-evasion)
- [Sensitive Data Disclosure (Updated)](https://saif.google/secure-ai-framework/risks#sensitive-data-disclosure)
- [Inferred Sensitive Data](https://saif.google/secure-ai-framework/risks#inferred-sensitive-data)
- [Insecure Model Output](https://saif.google/secure-ai-framework/risks#insecure-model-output)
- [Rogue Actions (Updated)](https://saif.google/secure-ai-framework/risks#rogue-actions)

* * *

### DP Data Poisoning

## Who can mitigate:

Model Creators


Altering data sources used during training or retraining (by deleting or modifying existing data as well as injecting adversarial data) to degrade model performance, skew results towards a specific outcome, or create hidden backdoors.

Data Poisoning can be considered comparable to maliciously modifying the logic of an application to change its behavior.

Data Poisoning attacks can happen during training or tuning, while data is held in storage, or even before the data is ingested into an organization. For example, foundation models are often trained on distributed web-scale datasets crawled from the Internet. An attack could [indirectly pollute a public data source](https://arxiv.org/pdf/2302.10149) that is eventually ingested. A malicious or compromised insider could also more directly poison the datasets while held in storage or during the training process, by submitting poisoned prompt-response examples for inclusion in the tuning data, as demonstrated in a 2023 research paper on [poisoning models during instruction tuning](https://arxiv.org/pdf/2305.00944.pdf).

Data Poisoning attacks can also install backdoors by specific alterations of the training data. Backdoored models would continue to function normally, but alternate behaviors could be triggered under certain conditions to make the model behave maliciously.

![](https://www.gstatic.com/marketing-cms/assets/images/1d/01/20df35b040c0b39dc95ad23e6b9b/dp-int.png=n-w543-h309-fcrop64=1,000001a1fffffe5f-rw)

Data Poisoning poses a risk throughout the data lifecycle. Data can be poisoned before it is ingested, during processing or training, or while the data is in storage. This makes it a critical concern across all data handling systems.

![](https://www.gstatic.com/marketing-cms/assets/images/bf/44/5804402444cd994b27a86bea4fef/dp-exp.png=n-w544-h305-fcrop64=1,00790000ff87ffff-rw)

Data Poisoning is exposed during development in the data filtering and processing steps or the training, tuning, and evaluation stages. It’s also exposed in the model itself, when it produces inaccurate results, malicious outputs, or unexpected behavior.

![](https://www.gstatic.com/marketing-cms/assets/images/b7/be/3800a49b415e9e3429b42605a1b7/dp-mit.png=n-w543-h311-fcrop64=1,00000316fffffdaf-rw)

Proactive mitigation against Data Poisoning happens early in development. This includes data sanitization, secure systems and access controls, and mechanisms to ensure data and model integrity.

1


/


#### Controls:

[Training Data Sanitization](https://saif.google/secure-ai-framework/controls#training-data-sanitization), [Secure-by-Default ML Tooling](https://saif.google/secure-ai-framework/controls#secure-by-default-ml-tooling), [Model and Data Integrity Management](https://saif.google/secure-ai-framework/controls#model-and-data-integrity-management), [Model and Data Access Control](https://saif.google/secure-ai-framework/controls#model-and-data-access-controls), [Model and Data Inventory Management](https://saif.google/secure-ai-framework/controls#model-and-data-inventory-management)

#### Real examples:

Researchers showed that they could [indirectly pollute popular data sources used for training models](https://arxiv.org/pdf/2302.10149) with minimal cost.


* * *

### UTD Unauthorized Training Data

#### Who can mitigate:

Model Creators


Training a model using data that is not authorized to be used for that model.

A model trained or fine tuned on unauthorized data could pose legal or ethical challenges. Unauthorized Training Data may include any data that violates policies, contracts, or regulations. Examples are user data that does not have appropriate user consent, unlicensed copyrighted data, or legally restricted data.

![](https://www.gstatic.com/marketing-cms/assets/images/0f/04/05ff0e4e40ee9d3b89733733f1b8/utd-int.png=n-w566-h305-fcrop64=1,05680000fafaffff-rw)

Unauthorized Training Data is introduced early in development if not properly filtered out during data ingestion, data processing, and model evaluation during training.

![](https://www.gstatic.com/marketing-cms/assets/images/98/ad/d061c4ab4966988f80bacd27f7c1/utd-exp.png=n-w543-h306-fcrop64=1,0000008cffffff74-rw)

The risk is exposed during development, through data filtering and processing steps or training, tuning, and evaluation. It is also exposed during model use, when the model may produce inferences based on data it shouldn’t have access to.

![](https://www.gstatic.com/marketing-cms/assets/images/35/87/de793a764befb39103bd261cca71/utd-mit.png=n-w543-h345-fcrop64=1,00000ed8fffff128-rw)

Mitigations for this risk start early, with careful data selection, filtering, and evaluation during training to catch any lingering issues.

1


/


#### Controls:

[Training Data Sanitization](https://saif.google/secure-ai-framework/controls#training-data-sanitization), [Training Data Management](https://saif.google/secure-ai-framework/controls#training-data-management)

#### Real examples:

In 2023, [Spotify removed multiple AI-generated tracks](https://aibusiness.com/ml/spotify-takes-down-thousands-of-ai-generated-tracks) that were generated by a model trained on unlicensed data.


* * *

### MST Model Source Tampering

#### Who can mitigate:

Model Creators


Tampering with the model’s source code, dependencies, or weights, either by supply chain attacks or insider attacks.

Similar to tampering with traditional software code, Model Source Tampering can introduce vulnerabilities or unexpected behaviors.

Since model source code is used in the process of developing the model, code modifications can affect model behavior. As with traditional code, attacks on a dependency can affect the program that relies on that dependency, so the risks in this area are transitive, potentially through many layers of a model code’s dependency chain.

Another method of Model Source Tampering is model architecture backdoors, which are [backdoors embedded within the definition of the neural network architecture.](https://arxiv.org/pdf/2402.06957.pdf) Such backdoors can survive full retraining of a model.

![](https://www.gstatic.com/marketing-cms/assets/images/18/1a/3b0090e84e21822da849b2dfaea0/mst-int.png=n-w574-h305-fcrop64=1,06e20000f8c6ffff-rw)

Model Source Tampering is a risk that’s introduced when model code, training frameworks, or model weights are not hardened against supply chain attacks and tampering.

![](https://www.gstatic.com/marketing-cms/assets/images/6b/65/262473b542c796817937632b4667/mst-exp.png=n-w557-h305-fcrop64=1,03340000fc71ffff-rw)

This risk is exposed in the model frameworks and code components, if the tampering is discovered at the source. Otherwise, the risk is exposed in the model, through its modified behavior during use.

![](https://www.gstatic.com/marketing-cms/assets/images/3e/d3/b252fe4349bdae3631610fb05034/mst-mit.png=n-w543-h306-fcrop64=1,0000008cffffff74-rw)

Safeguard against this risk by employing robust access controls and integrity management for model code and weights, comprehensive inventory tracking to monitor and verify models and code throughout systems, and secure-by-default infrastructure tools.

1


/


#### Controls:

[Secure-by-Default ML Tooling](https://saif.google/secure-ai-framework/controls#secure-by-default-ml-tooling), [Model and Data Integrity Management](https://saif.google/secure-ai-framework/controls#model-and-data-integrity-management), [Model and Data Access Control](https://saif.google/secure-ai-framework/controls#model-and-data-access-controls), [Model and Data Inventory Management](https://saif.google/secure-ai-framework/controls#model-and-data-inventory-management)

#### Real examples:

The nightly build of [PyTorch package was subjected to a supply chain attack](https://pytorch.org/blog/compromised-nightly-dependency/) (specifically, a dependency confusion attack that installed a compromised dependency that ran a malicious binary).


* * *

### EDH Excessive Data Handling

#### Who can mitigate:

Model Creators


Collection, retention, processing, or sharing of user data beyond what is allowed by relevant policies.

Excessive Data Handling can create both policy and legal challenges.

In the context of models, user data might include user queries, text inputs and interactions, personalizations and preferences, and models derived from such data.

![](https://www.gstatic.com/marketing-cms/assets/images/7c/91/43098ef4477fa1f0f3e36ff64df8/edh-int.png=n-w546-h305-fcrop64=1,00b70000ff49ffff-rw)

The risk of Excessive Data Handling is introduced when data sources lack proper metadata tagging for effective management or when model and data storage infrastructure isn't designed to address data lifecycle concerns.

![](https://www.gstatic.com/marketing-cms/assets/images/62/ab/112aa88c49c0805d5aea25acadc7/edh-exp.png=n-w543-h370-fcrop64=1,00001693ffffe96d-rw)

This risk is exposed in both the model and in storage components, leading to data retention or usage beyond permissible limits.

![](https://www.gstatic.com/marketing-cms/assets/images/35/28/1223260b45a4916b7f3db2786884/edh-mit.png=n-w543-h346-fcrop64=1,00000f76fffff08a-rw)

Mitigate this risk with data filtering and processing, along with automation for data archiving, deletion, or issuing alerts for models trained with outdated data.

1


/


#### Controls:

[User Data Management](https://saif.google/secure-ai-framework/controls#user-data-management)

#### Real examples:

[Samsung banned usage of ChatGPT](https://www.forbes.com/sites/siladityaray/2023/05/02/samsung-bans-chatgpt-and-other-chatbots-for-employees-after-sensitive-code-leak/) after discovering private code source has leaked via using it in GenAI prompts.


* * *

### MXF Model Exfiltration

#### Who can mitigate:

Model Creators, Model Consumers


Unauthorized appropriation of an AI model, for replicating functionality or to extract intellectual property.

Similar to stealing code, this threat has intellectual property, security, and privacy implications.

For example, someone could hack into a cloud environment and steal a generative AI model; the model size when serialized is fairly modest and not a major obstacle for this. Models, and related data such as weights, are also at risk of theft in the internal development, build, deployment, and production environments by insiders and external attackers that have taken over privileged insider accounts.

These risks also extend to on-device models, where an attacker has access to hardware.

This risk is distinct from the related [Model Reverse Engineering](https://saif.google/secure-ai-framework/risks#model-reverse-engineering).

![](https://www.gstatic.com/marketing-cms/assets/images/f9/7d/42b6118749408d8ea86b15c896cc/mxf-int.png=n-w543-h306-fcrop64=1,0000008cffffff74-rw)

Model Exfiltration is introduced when storage or serving infrastructure lacks adequate security against attacks.

![](https://www.gstatic.com/marketing-cms/assets/images/ed/c6/213a08fa4f35a09d7297317f0b44/mxf-exp.png=n-w543-h306-fcrop64=1,0000008cffffff74-rw)

This risk is exposed if attackers target vulnerabilities in serving or storage systems to steal model code or weights.

![](https://www.gstatic.com/marketing-cms/assets/images/f5/bc/c8e9d39c4e7ca33c5d98063458a7/mxf-mit.png=n-w569-h305-fcrop64=1,05f20000fa0effff-rw)

Mitigate this risk by hardening both storage and serving systems to prevent unauthorized access and protect against model theft.

1


/


#### Controls:

[Model and Data Inventory Management](https://saif.google/secure-ai-framework/controls#model-and-data-inventory-management), [Model and Data Access Control](https://saif.google/secure-ai-framework/controls#model-and-data-access-controls), [Secure-by-Default ML Tooling](https://saif.google/secure-ai-framework/controls#secure-by-default-ml-tooling)

#### Real examples:

[Meta's Llama model was leaked online](https://www.theverge.com/2023/3/8/23629362/meta-ai-language-model-llama-leak-online-misuse), bypassing Meta's license acceptance review process.


* * *

### MDT Model Deployment Tampering

#### Who can mitigate:

Model Creators, Model Consumers


Unauthorized modification of components used for deploying a model, whether by tampering with the source code supply chain or exploiting known vulnerabilities in common tools.

Such modifications can result in changes to model behavior.

One type of Model Deployment Tampering is candidate model modification where the attacker is modifying the deployment workflow or processes to maliciously alter the way the model operates post-deployment.

A second type is compromise of the model serving infrastructure. For example, it was reported that [PyTorch models were vulnerable to remote code execution](https://thehackernews.com/2023/10/warning-pytorch-models-vulnerable-to.html) due to multiple critical security flaws in the **TorchServe** tool that is widely used for serving the models. This is an attack on a serving infrastructure for PyTorch, TorchServe, whereas the [PyTorch example of Model Source Tampering](https://pytorch.org/blog/compromised-nightly-dependency/) was about a supply chain attack on dependency code for PyTorch itself.

![](https://www.gstatic.com/marketing-cms/assets/images/92/9b/5d5376d84974bd76420b2bcc6dad/mdt-int.png=n-w543-h316-fcrop64=1,0000044afffffb19-rw)

The risk of Model Deployment Tampering is introduced within the model serving components, specifically when the serving infrastructure is vulnerable to manipulation.

![](https://www.gstatic.com/marketing-cms/assets/images/5f/0a/4791c86a454f9dae91c49be1f6e6/mdt-exp.png=n-w543-h316-fcrop64=1,000004f0fffffb10-rw)

This risk is exposed if attackers tamper with production models within the model serving component.

![](https://www.gstatic.com/marketing-cms/assets/images/6a/9c/d57907624848bd0369e80d50c968/mdt-mit.png=n-w543-h306-fcrop64=1,000000a1ffffff5f-rw)

Mitigation focuses on hardening the model serving infrastructure with with secure-by-default tooling.

1


/


#### Controls:

[Secure-by-Default ML Tooling](https://saif.google/secure-ai-framework/controls#secure-by-default-ml-tooling)

#### Real examples:

[Researchers discovered that models on HuggingFace were using a shared infrastructure for inference](https://www.wiz.io/blog/wiz-and-hugging-face-address-risks-to-ai-infrastructure#what-did-we-find-11), which allowed a malicious model to tamper with any other model.


* * *

### DMS Denial of ML Service

#### Who can mitigate:

Model Consumers


Reducing the availability of ML systems and denying service by issuing queries that take too many resources.

Examples of attacks include traditional Denial of Service or spamming a system with abusive material to overload automated or manual review processes. If an API-gated model does not have appropriate rate limiting or load balancing, the repeated queries can take the model offline, making it unavailable to other users.

There are also [energy-latency attacks](https://arxiv.org/pdf/2006.03463.pdf): attackers can carefully craft “sponge examples” (also known as queries of death), which are inputs designed to maximize energy consumption and latency, pushing ML systems towards their worst-case performance. Adversaries might use their own tools to accelerate construction of such sponge examples. These attacks are especially relevant for on-device models, since the increased energy consumption can drain batteries and make the model unavailable.

![](https://www.gstatic.com/marketing-cms/assets/images/55/04/4cdb4993412e8d89b8b255beee27/dms-int.png=n-w543-h308-fcrop64=1,00000142fffffebe-rw)

The risk of Denial of ML Service arises in the application component when a model is exposed to excessive access. Additionally, some types of Denial of ML Service (such as energy-latency attacks) stem from the fundamental functioning of the model itself.

![](https://www.gstatic.com/marketing-cms/assets/images/5e/1c/ad59b7794b57b0159aecc7a03b16/dms-exp.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

This risk is exposed during application use, when attackers either overwhelm the model with excessive calls or use carefully crafted "sponge examples" that take advantage of model weaknesses to degrade performance.

![](https://www.gstatic.com/marketing-cms/assets/images/d8/ac/4c2422ae4622b8fbaf2b3679b42e/dms-mit.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

Mitigation occurs at the application level, using input filtering and employing rate limiting and load balancing to control the volume of calls to the model.

1


/


#### Controls:

[Application Access Management](https://saif.google/secure-ai-framework/controls#application-access-management)

#### Real examples:

Researchers have proven how slight perturbation to images [can cause denial of service on object detection models](https://arxiv.org/pdf/2205.13618).


* * *

### MRE Model Reverse Engineering

#### Who can mitigate:

Model Consumers


Cloning or recreating a model by analyzing a model's inputs, outputs, and behaviors.

The stolen or cloned model can be used for building imitation products or developing [adversarial attacks](https://arxiv.org/pdf/2004.15015) on the original model.

If a model API does not have rate limits, one method of Model Reverse Engineering is repeatedly calling the API to gather responses in order to create a dataset of thousands of input/output pairs from a target LLM. This dataset can be leveraged to reconstruct a copycat or distilled model more cheaply than developing the original foundation model.

These risks also extend to on-device models, where an attacker has access to hardware. See also [Model Exfiltration](https://saif.google/secure-ai-framework/risks#model-exfiltration).

![](https://www.gstatic.com/marketing-cms/assets/images/30/1f/c53f49cf4ef3995446346d1dd339/mre-int.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

The risk of Model Reverse Engineering arises within the application component when excessive access to the model is granted for queries.

![](https://www.gstatic.com/marketing-cms/assets/images/20/15/4d2b8bfa4125a9f42ee68424b91c/mre-exp.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

This risk is exposed if attackers send excessive queries to the model and leverage the responses to reverse engineer its weights.

![](https://www.gstatic.com/marketing-cms/assets/images/7d/bb/779315214ef5ba1abdd1ee23c462/mre-mit.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

Mitigate this risk with rate limiting within the application API or using other protective measures at the application level to prevent excessive model access.

1


/


#### Controls:

[Application Access Management](https://saif.google/secure-ai-framework/controls#application-access-management)

#### Real examples:

A Stanford University research team created [Alpaca 7B](https://crfm.stanford.edu/2023/03/13/alpaca.html), a model fine-tuned from the LLaMA 7B model based on 52,000 instruction-following examples.


* * *

### IIC Insecure Integrated Component

#### Who can mitigate:

Model Consumers


Vulnerabilities in software interacting with AI models, such as a plugin, library, or application, that can be leveraged by attackers to gain unauthorized access to models, introduce malicious code, or compromise system operations.

Given the level of autonomy expected to be granted to agents and applications, insecure integrated components represent a broad swath of threats to user trust and safety, privacy and security concerns, and ethical and legal challenges.

This risk can come from manipulation of both inputs to and outputs from integrations:

- Manipulation of **model output** to include malicious instructions fed as **input to the integrated component or system**. For example, a plugin that accepts freeform text instead of structured and validated input could be exploited to construct inputs that cause the plugin to behave maliciously. Likewise, a plugin that accepts input without authentication and authorization can be exploited since it trusts input as coming from an authorized user.

- Manipulation of the **output from an integrated component or system** that is fed as **input to a model.** For example, when a plugin calls other systems, especially 3rd party services, sites, or plugins, and uses content obtained from those to construct output to a model, opening up potential for indirect prompt injection. A similar case exists for an integrated application that calls another service and uses content from that service to construct an input to a model.

Insecure Integrated Component is related to [Prompt Injection](https://saif.google/secure-ai-framework/risks#prompt-injection) but these are different. Although attacks exploiting an Insecure Integrated Component often involve prompt injection, those could be also done via other means such as Poisoning and Evasion. In addition, prompt injection is possible even when the integrated components are secure.

![](https://www.gstatic.com/marketing-cms/assets/images/58/da/9d2981ec4da087487cc23adba2f5/iic-int.png=n-w665-h305-fcrop64=1,17b00000e8aaffff-rw)

The risk of Insecure Integrated Components is introduced in the application and agent components, specifically through integrations that permit manipulation of inputs or outputs.

![](https://www.gstatic.com/marketing-cms/assets/images/a9/b3/5d80c9924a58842a8f1f56b9f329/iic-exp.png=n-w661-h305-fcrop64=1,16d20000e8d3ffff-rw)

This risk is exposed within the application or agent components, if attackers exploit the security vulnerability to gain unauthorized model access, insert malicious code, or compromise systems.

![](https://www.gstatic.com/marketing-cms/assets/images/87/f2/6e17cd744371897be2931c7061bc/iic-mit.png=n-w615-h305-fcrop64=1,0f450000f116ffff-rw)

Mitigate this risk by addressing vulnerabilities directly within the application and agent components, and by enforcing strict permissions for agents and plugins.

1


/


#### Controls:

[Agent Permissions](https://saif.google/secure-ai-framework/controls#agent-permissions)

#### Real examples:

By uploading a malicious Alexa skill / Google action (plugins), [attackers were able to eavesdrop on user conversations](https://www.theverge.com/2019/10/21/20924886/alexa-google-home-security-vulnerability-srlabs-phishing-eavesdropping) that occurred near Alexa / Google Home devices.


* * *

### PIJ Prompt Injection

#### Who can mitigate:

Model Creators, Model Consumers


Causing a model to execute commands “injected” inside a prompt.

Prompt Injection takes advantage of the blurry boundary between “instructions” and “input data” in a prompt, resulting in a change to the model’s behavior. These attacks can be both direct (entered directly by the user) or indirect (read from other sources such as a doc, email, or website).

[Jailbreaks](https://arxiv.org/pdf/2307.02483.pdf) are one type of Prompt Injection attack, causing the model to behave in ways that they’ve been trained to avoid, such as outputting unsafe content or leaking personally identifiable information. These are well-known vulnerabilities such as "ignore your previous instructions" or “Do Anything Now” (DAN).

Aside from jailbreaks, [Prompt Injections](https://arxiv.org/pdf/2302.12173.pdf) generally cause the LLM to execute malicious “injected” instructions as part of data that were not meant to be executed by the LLM. The blast radius of such attacks can become much bigger in the presence of other risks such as [Insecure Integrated Component](https://saif.google/secure-ai-framework/risks#insecure-integrated-component) and [Rogue Actions](https://saif.google/secure-ai-framework/risks#rogue-actions).

With foundation models becoming multi-modal, multi-modal prompt injection has also become possible. These attacks use injection inputs other than text to trigger the intended model behavior.

![](https://www.gstatic.com/marketing-cms/assets/images/6d/c6/5511b4f1423abd553b44c2ea34d2/pij-int.png=n-w562-h305-fcrop64=1,04430000fb62ffff-rw)

Prompt Injection is an inherent risk in AI models, because of the potential confusion between instructions and input data.

![](https://www.gstatic.com/marketing-cms/assets/images/b2/fd/b04616444bae9c4ad8c02b0bbf77/pij-exp.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

This risk is exposed during model usage, specifically within the model input handling and model components. Attackers may inject commands within prompts, potentially causing unintended model actions.

![](https://www.gstatic.com/marketing-cms/assets/images/a4/a6/15031e004a19b82eb423c0ea502e/pij-mit.png=n-w543-h315-fcrop64=1,00000473fffffc30-rw)

Mitigation involves robust filtering and processing of inputs and outputs. Additionally, thorough training, tuning, and evaluation processes help fortify the model against prompt injection attacks.

1


/


#### Controls:

[Input Validation and Sanitization](https://saif.google/secure-ai-framework/controls#input-validation-and-sanitization), [Adversarial Training and Testing](https://saif.google/secure-ai-framework/controls#adversarial-training-and-testing), [Output Validation and Sanitization](https://saif.google/secure-ai-framework/controls#output-validation-and-sanitization)

#### Real examples:

An example of indirect Prompt Injection was performed by [planting malicious data inside a resource fed into the LLM’s prompt](https://arxiv.org/pdf/2302.12173). In another example, a [multi-modal prompt injection image attacks against GPT-4V](https://simonwillison.net/2023/Oct/14/multi-modal-prompt-injection/) showed that images can contain text that triggers a Prompt Injection attack when the model is asked to describe the image.


* * *

### MEV Model Evasion

#### Who can mitigate:

Model Creators, Model Consumers


Causing a model to produce incorrect inferences by slightly perturbing the prompt input.

Model Evasion can result in reputational or legal challenges and trigger other downstream risks, such as to security or privacy systems.

A classic example is placing stickers on a stop sign to obscure the visual inputs to model piloting a self-driving car. Because of the change to the typical visual presentation of the sign, the model might not correctly infer its presence. Similarly, normal wear and tear on a stop sign could lead to misidentification if the model is not trained on images of signs in varying degrees of disrepair.

In some cases, an attacker might gain clues about how to perturb inputs by discovering the underlying foundation model’s family, i.e., by knowing the particular architecture and evolution of a specific model. In other situations, an attacker might repeatedly probe the model (see [Model Reverse Engineering](https://saif.google/secure-ai-framework/risks#model-reverse-engineering)) to figure out inference patterns in order to craft examples that evade those inferences. Adversarial examples might be constructed by perturbations to inputs that will provide the output the attacker wants while looking unaltered otherwise. This could be used, for example, for evading a classifier that serves as an important safeguard.

Not all examples of model evasion attacks are necessarily visible to the naked eye. The inputs might be perturbed in such a way to appear unaltered, but still produce the output the attacker wants. For example, a homoglyph attack involves slight changes to typefaces that the human eye doesn’t perceive as a different letter, but could trigger unexpected inferences in the mode. Another example could be sending an image in the prompt but using steganography to encode text within the image pixels. This text would be part of the prompt for the LLM, but the user won’t see it.

![](https://www.gstatic.com/marketing-cms/assets/images/bf/f3/59ca0bd84243875ed6782b3df9e0/mev-int.png=n-w543-h311-fcrop64=1,00000285fffffd7b-rw)

Model Evasion is an inherent risk in AI models, as their core functionality relies on distinguishing between inputs to trigger specific inferences.

![](https://www.gstatic.com/marketing-cms/assets/images/71/fb/014b99d04ee3a96eaa8fae0f3507/mev-exp.png=n-w543-h322-fcrop64=1,00000732fffff967-rw)

This risk is exposed within the model component itself during its usage.

![](https://www.gstatic.com/marketing-cms/assets/images/70/0c/4f7333c54bdabcbddc2fe8f26d35/mev-mit.png=n-w543-h334-fcrop64=1,00000b7bfffff518-rw)

Mitigation occurs in the training, tuning, and evaluation phases, where robust models can be developed using extensive and diverse data to better withstand such attacks.

1


/


#### Controls:

[Adversarial Training and Testing](https://saif.google/secure-ai-framework/controls#adversarial-training-and-testing)

#### Real examples:

Adversarial images have been used to [modify street signs to confuse self-driving cars](https://spectrum.ieee.org/slight-street-sign-modifications-can-fool-machine-learning-algorithms).


* * *

### SDD Sensitive Data Disclosure

#### Who can mitigate:

Model Creators, Model Consumers


Disclosure of private or confidential data through querying of the model or agent.

For non-agentic systems, this data might include memorized training/tuning data, user chat history, and confidential data in the prompt preamble. Agentic systems magnify this risk, as they may be granted privileged access to a user's email, files, or even an entire computer, creating the potential to exfiltrate vast amounts of personal or corporate data like source code and internal documents. Sensitive data disclosure is a risk to user privacy, organizational reputation, and intellectual property.

Sensitive information is generally disclosed in two ways: leakage of data provided to the model or agent during use (such as user input and data that passes through integrated systems, like emails, texts, or system prompts) and leakage of data used for training and tuning of the model.

- **Models**: Models can leak sensitive data in two primary ways: from the information provided by the user and from the data used for the model's own training. Similar to how a leaked web query can reveal user information, LLM prompts risk data leakage at time of use, a threat that is heightened because prompts often contain confidential data like entire emails or blocks of proprietary code. This exposure can occur through several vectors: application logs may store entire interactions, including data retrieved from integrated tools, and user conversations may be retained for model retraining, creating a vulnerable database of sensitive information. Beyond leaking user-provided data, attackers can actively [steal system instructions through iterative queries](https://arxiv.org/abs/2307.06865), or a model may inadvertently leak the data it was trained on. This phenomenon, known as memorization, occurs when a model reveals parts of its training dataset, potentially exposing sensitive information like names, addresses, or other personally identifiable information (PII).
- **Agents**: For agentic systems, the risk of sensitive data disclosure is exponentially multiplied, since agents may access user data that passes through integrated systems, like emails, texts, or proprietary organizational information. In extreme cases, agents can even reveal credentials and API keys they have been trusted with. Additionally, agents may use tools to not only access sensitive data on behalf of the user, but also use those tools to leak sensitive data. For example, an agent can leak information by creating and sharing a document with an attacker, writing an email, opening a website and leaking information in the URL [or a markdown image](https://www.aim.security/aim-labs/aim-labs-echoleak-blogpost), or through any tool that allows it to pass information to the outside world. [Context-hijacking attacks](https://arxiv.org/abs/2405.05175) show that an adversary can confuse the agent to reveal data that is not appropriate for a specific context, such as sharing health history when the agent should be booking a restaurant reservation.

![](https://www.gstatic.com/marketing-cms/assets/images/2c/83/f599eeb548608d97a0c5d076e863/sdd-introduced.png=n-w543-h325-fcrop64=1,000007c7fffff7f2-rw)

The risk of Sensitive Data Disclosure is introduced in several components. It can also be inherent to models due to their non-deterministic nature. This risk is amplified by data handling practices that fail to filter sensitive information, or by training processes that neglect to evaluate the model's potential for disclosure. In agentic contexts, the risk is introduced when an agent uses its privileged access and integrated tools to disclose sensitive information from a user's emails, files, or other connected systems.

![](https://www.gstatic.com/marketing-cms/assets/images/49/4b/16cfadc9466e92606095193b201f/sdd-exposed.png=n-w543-h325-fcrop64=1,00000810fffff837-rw)

This risk is exposed within the application, when the model inadvertently reveals sensitive data it shouldn't. In agentic systems, the risk is magnified when an agent uses its privileged access and integrated tools to reveal sensitive data in interactions with a third party.

![](https://www.gstatic.com/marketing-cms/assets/images/7a/ba/9c3534494149a6c90eec2562d494/sdd-mitigated.png=n-w543-h324-fcrop64=1,000007c0fffff7f9-rw)

Mitigate sensitive data disclosure by: filtering model outputs, rigorously testing the model during training, tuning, and evaluation, and removing or labeling sensitive data during sourcing, filtering, and processing before it's used for training. For agents, implement controls at multiple levels of the system, including enforcing permissions on the agent’s access to tools and sensitive data, safely rendering agent outputs, and using application-level warnings to get user confirmation before executing actions that may disclose sensitive information.

1


/


#### Controls:

[Privacy Enhancing Technologies](https://saif.google/secure-ai-framework/controls#privacy-enhancing-technologies), [User Data Management](https://saif.google/secure-ai-framework/controls#user-data-management), [Output Validation and Sanitization](https://saif.google/secure-ai-framework/controls#output-validation-and-sanitization), [Agent Permissions](https://saif.google/secure-ai-framework/controls#agent-permissions), [Agent User Control](https://saif.google/secure-ai-framework/controls#agent-user-control), [Agent Observability](https://saif.google/secure-ai-framework/controls#agent-observability)

#### Real examples:

One study showed that [recitation checkers that scan for verbatim repetition of training data](https://arxiv.org/pdf/2210.17546) may be insufficient.


An example of [membership inference attacks](https://arxiv.org/pdf/1610.05820.pdf) showed the possibility of inferring whether a specific user or data point was used to train or tune the model.


* * *

### ISD Inferred Sensitive Data

#### Who can mitigate:

Model Creators, Model Consumers


Models inferring sensitive information about people that is not contained in the model’s training data.

Inferred information that turns out to be true, even if produced as part of a hallucination, can be considered a data privacy incident, whereas the same information when false would be treated as a factuality issue.

For example, a model may be able to infer information about people (gender, political affiliation, or sexual orientation) based on their inputs and responses from integrated plugins, such as a social media plugin that accesses a public account’s liked pages or followed accounts. Though the data used for inference may be public, this type of inference poses two related risks: that a user may be alarmed if a model infers sensitive data about them, and that one user may use a model to infer sensitive data about someone _else._

This risk differs from [Sensitive Data Disclosure](https://saif.google/secure-ai-framework/risks#sensitive-data-disclosure) which involves sensitive data specifically from training, tuning or prompt data.

![](https://www.gstatic.com/marketing-cms/assets/images/eb/f5/99f02fe94f14ae14ee48d3ba54c8/isd-int.png=n-w543-h334-fcrop64=1,00000b54fffff4ac-rw)

The risk of Inferred Sensitive Data is introduced in several components. It's inherent to models due to their non-deterministic nature and is amplified by inadequate data handling practices that fail to filter sensitive information. It can also be due to training processes that neglect to evaluate the model's potential for sensitive inferences.

![](https://www.gstatic.com/marketing-cms/assets/images/e9/18/33dc056948a29ab0201f2b2a162c/isd-exp.png=n-w543-h318-fcrop64=1,0000058afffffa76-rw)

This risk is exposed within the model when it generates a response containing inferred sensitive data that it shouldn't.

![](https://www.gstatic.com/marketing-cms/assets/images/40/af/b4606083444f92d07aed4b45829c/isd-mit.png=n-w543-h305-fcrop64=1,00000000ffffff8f-rw)

Mitigation is multi-pronged: filtering model outputs to prevent revealing inferred sensitive data, rigorously testing the model during training, tuning, and evaluation to prevent sensitive inferences, and proactively removing or labeling data that could lead to such inferences during sourcing, filtering, and processing before training.

1


/


#### Controls:

[Training Data Management](https://saif.google/secure-ai-framework/controls#training-data-management), [Output Validation and Sanitization](https://saif.google/secure-ai-framework/controls#output-validation-and-sanitization)

#### Real examples:

Examples include papers on [AI inferences about sexual orientation](https://osf.io/preprints/psyarxiv/hv28a) or [criminal record from faces](https://confilegal.com/wp-content/uploads/2016/11/ESTUDIO-UNIVERSIDAD-DE-JIAO-TONG-SHANGHAI.pdf).


* * *

### IMO Insecure Model Output

#### Who can mitigate:

Model Consumers


Model output that is not appropriately validated, rewritten, or formatted before being passed to downstream systems or the user.

Whether accidentally triggered or actively exploited, Insecure Model Output poses risks to organizational reputation, security, and user safety.

For example, a user who asks an LLM to generate an email for their business’s promotion would be harmed if the model produces text that unexpectedly includes a link to a URL that delivers malware. Alternatively, a malicious actor could intentionally trigger insecure content, such as requesting the LLM to produce a phishing email based on specific details about the target.

![](https://www.gstatic.com/marketing-cms/assets/images/44/87/27a854804b989782f29a1f0d59d3/imo-int.png=n-w543-h305-fcrop64=1,00000000ffffff73-rw)

The risk of Insecure Model Output is inherent to AI models due to their non-deterministic nature, which can lead to unexpected and potentially harmful outputs.

![](https://www.gstatic.com/marketing-cms/assets/images/c9/6a/fc1e3610496f8dc430e95103880b/imo-exp.png=n-w543-h317-fcrop64=1,0000056cfffffb2e-rw)

This risk is exposed within the model itself during usage, either through accidental triggers or deliberate exploitation.

![](https://www.gstatic.com/marketing-cms/assets/images/34/f9/f8f75f1e46a2837bea9211df90d1/imo-mit.png=n-w543-h307-fcrop64=1,0000013ffffffec1-rw)

Mitigation includes robust model validation and sanitization processes within the model output handling component to screen and filter for insecure responses.

1


/


#### Controls:

[Output Validation and Sanitization](https://saif.google/secure-ai-framework/controls#output-validation-and-sanitization)

#### Real examples:

[Attackers can compromise users by creating fake malicious packages with names inspired by LLM hallucinations](https://www.theregister.com/2024/03/28/ai_bots_hallucinate_software_packages/).


* * *

### RA Rogue Actions

#### Who can mitigate:

Model Consumers


Unintended actions executed by a model-based agent, whether accidental or malicious. Given the projected ability for advanced generative AI models to not only understand their environment, but also to initiate actions with varying levels of autonomy, Rogue Actions have the potential to become a serious risk to organizational reputation, user trust, security, and safety.

- Accidental rogue actions: This risk, sometimes known as misalignment, could be due to mistakes in task planning, reasoning, or environment sensing, and might be exacerbated by the inherent variability in LLM responses. Prompt engineering shows the spacing and ordering of examples can have a significant impact on the response, so varying input (even when not maliciously planted) could result in unexpected outcomes. [Even simple ambiguity](https://www.arxiv.org/abs/2506.12241) can cause rogue actions, such as an agent emailing the wrong "Mike," unintentionally sharing private data.
- Malicious rogue actions: This risk could include manipulating model output using attacks such as indirect prompt injection, poisoning, or evasion. The threat can be amplified in multi-agent systems, where the attacker can [hijack the communication between two agents](https://arxiv.org/pdf/2503.12188) to execute arbitrary malicious code, even if the individual agents are secured against direct attacks. Malicious actions may also be asynchronous. An attacker can plant a dormant "named trigger" that activates later during an unrelated task—for instance, a rule hidden [in a calendar invite](https://www.wired.com/story/google-gemini-calendar-invite-hijack-smart-home/) that opens the front door whenever the user says an unrelated keyword. Other actions may be time-based, occurring after a set number of interactions, making the rogue action appear spontaneous and disconnected from the malicious source.

Rogue Actions are related to [Insecure Integrated Components](https://saif.google/secure-ai-framework/risks#insecure-integrated-component), but differ by the degree of model functionality or agency. The severity of a rogue action is directly proportional to the agent's capabilities, and the possibility that an agent has excessive functionality or permissions available to it increases the risk and blast radius of Rogue Actions when compared to Insecure Integrated Components.

![](https://www.gstatic.com/marketing-cms/assets/images/69/14/f139bdc84f22950c8ed021f079ae/ra-introduced.png=n-w543-h324-fcrop64=1,000007cbfffff835-rw)

Integrating agents into an AI system introduces the risk of Rogue Actions by dramatically expanding the model's ability to trigger real-world actions and consequences. This risk is introduced through a failure of the reasoning core to align with the user’s intent, or through the poisoning of orchestration components like tools, memory, and retrieved data.

![](https://www.gstatic.com/marketing-cms/assets/images/59/af/6a7e38ce47bcb40e9ddea1e973c6/ra-exposed.png=n-w543-h324-fcrop64=1,000007cbfffff835-rw)

This vulnerability is exposed during application usage when the model inadvertently triggers an unintended action.

![](https://www.gstatic.com/marketing-cms/assets/images/44/42/99a74ecc4877a0513e0bec3c837b/ra-mitigated.png=n-w543-h324-fcrop64=1,000007cbfffff835-rw)

Mitigating Rogue Actions requires a multi-layered defense, starting with filtering and standardizing all inputs before they reach the model and defining tool limitations in the agent’s system instructions. Harden the reasoning core and model themselves with adversarial training to recognize prompt injection. Within orchestration, govern the agent’s capabilities with observability, policy engines, and credentialed tool access, implementing contextual agent security. Finally, normalize and sanitize outputs during rendering, and implement user-facing notifications and other safeguards tailored to the specific application or platform to prevent exploitation.

1


/


#### Controls:

[Agent Permissions](https://saif.google/secure-ai-framework/controls#agent-permissions), [Agent User Control](https://saif.google/secure-ai-framework/controls#agent-user-control), [Agent Observability](https://saif.google/secure-ai-framework/controls#agent-observability), [Output Validation and Sanitization](https://saif.google/secure-ai-framework/controls#output-validation-and-sanitization)

#### Real examples:

An attack on ChatGPT plugins was described in [Plugin Vulnerabilities: Visit a Website and Have Your Source Code Stolen](https://embracethered.com/blog/posts/2023/chatgpt-plugin-vulns-chat-with-code/).


## Footer links

[Google](https://saif.google/ "Google")

- [Privacy](https://policies.google.com/privacy?hl=en)
- [Terms](https://policies.google.com/terms?hl=en)
- [Feedback](mailto:saif-feedback@google.com)
- Cookies management controls

The content on this site is intended to provide information and inspiration for industry advancement. It is not a reflection of Google's current technical implementations.