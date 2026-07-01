///usr/bin/env jbang "$0" "$@" ; exit $?
// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//
// Validator-zoo probe for the openEHR blood-pressure in-band-complement claim — Archie RM
// validator lane (the second animal in the zoo, alongside the EHRbase probe.sh CDR lane).
//
// Loads Blutdruck.opt via Archie's JAXB (un)marshaller into an OPERATIONAL_TEMPLATE, then
// parses each of the vendored source/augmented compositions with Archie's standards-compliant
// Jackson mapper and runs com.nedap.archie.rmobjectvalidator.RMObjectValidator over each
// against the template. Prints exactly one of:
//   PASS source+augmented validate under Blutdruck.opt
//   BOUNDARY <detail>
// and exits non-zero on BOUNDARY. See README.md "Option B".
//
//DEPS com.nedap.healthcare.archie:archie-all:3.14.0

import com.nedap.archie.aom.OperationalTemplate;
import com.nedap.archie.flattener.InMemoryFullArchetypeRepository;
import com.nedap.archie.json.JacksonUtil;
import com.nedap.archie.rm.composition.Composition;
import com.nedap.archie.rminfo.ArchieRMInfoLookup;
import com.nedap.archie.rmobjectvalidator.RMObjectValidationMessage;
import com.nedap.archie.rmobjectvalidator.RMObjectValidator;
import com.nedap.archie.rmobjectvalidator.ValidationConfiguration;
import com.nedap.archie.xml.JAXBUtil;

import javax.xml.bind.Unmarshaller;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.util.List;

public class archie_probe {

    public static void main(String[] args) throws Exception {
        if (args.length != 3) {
            System.err.println("usage: archie_probe.java <Blutdruck.opt> <source.json> <augmented.json>");
            System.exit(2);
        }
        File optFile = new File(args[0]);
        File sourceFile = new File(args[1]);
        File augmentedFile = new File(args[2]);

        OperationalTemplate opt;
        try (InputStream in = new FileInputStream(optFile)) {
            Unmarshaller unmarshaller = JAXBUtil.getArchieJAXBContext().createUnmarshaller();
            Object parsed = unmarshaller.unmarshal(in);
            if (!(parsed instanceof OperationalTemplate)) {
                boundary("opt-parse: " + optFile + " did not unmarshal to an OPERATIONAL_TEMPLATE (got "
                        + parsed.getClass().getName() + ")");
                return;
            }
            opt = (OperationalTemplate) parsed;
        } catch (Exception e) {
            boundary("opt-parse: " + optFile + " failed to parse: " + e);
            return;
        }

        InMemoryFullArchetypeRepository repo = new InMemoryFullArchetypeRepository();
        repo.setOperationalTemplate(opt);

        RMObjectValidator validator = new RMObjectValidator(
                ArchieRMInfoLookup.getInstance(), repo,
                new ValidationConfiguration.Builder().build());

        validateComposition(validator, opt, "source", sourceFile);
        validateComposition(validator, opt, "augmented", augmentedFile);

        System.out.println("PASS source+augmented validate under Blutdruck.opt");
    }

    private static void validateComposition(RMObjectValidator validator, OperationalTemplate opt,
                                              String label, File file) throws Exception {
        Composition composition;
        try (InputStream in = new FileInputStream(file)) {
            composition = JacksonUtil.getObjectMapper().readValue(in, Composition.class);
        } catch (Exception e) {
            boundary(label + ": " + file + " failed to parse as a COMPOSITION: " + e);
            return;
        }

        List<RMObjectValidationMessage> messages = validator.validate(opt, composition);
        if (!messages.isEmpty()) {
            RMObjectValidationMessage first = messages.get(0);
            boundary(label + ": " + first.getPath() + " (archetype path " + first.getArchetypePath()
                    + "): " + first.getMessage());
        }
    }

    private static void boundary(String detail) {
        System.out.println("BOUNDARY " + detail);
        System.exit(1);
    }
}
