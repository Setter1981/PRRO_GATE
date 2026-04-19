using System;
using System.Net;
using System.Net.Mail;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SendingMail
{
	public TypMail FrM { get; set; }

	internal bool SendMail(bool ini, string ToMail, string Tema, string Body, bool MailAsync = false)
	{
		bool result;
		try
		{
			TypMail typMail = ((!ini) ? FrM : LoadEmail());
			MailMessage mailMessage = new MailMessage();
			mailMessage = new MailMessage();
			mailMessage.From = new MailAddress(typMail.From);
			mailMessage.To.Add(ToMail);
			mailMessage.Subject = Tema;
			mailMessage.Body = Body;
			SmtpClient smtpClient = new SmtpClient();
			smtpClient.Port = typMail.Port;
			smtpClient.Host = typMail.Host;
			smtpClient.Timeout = typMail.TimeOut;
			smtpClient.EnableSsl = typMail.EnableSSL;
			smtpClient.UseDefaultCredentials = typMail.UseDefaultCredentials;
			smtpClient.Credentials = new NetworkCredential(typMail.From, typMail.Pass);
			if (MailAsync)
			{
				smtpClient.SendMailAsync(mailMessage);
			}
			else
			{
				smtpClient.Send(mailMessage);
			}
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypMail LoadEmail()
	{
		TypMail result = default(TypMail);
		result.ErrCode = 0;
		result.From = All.f.StringGetFn(All.A.FN, "eMail");
		if (Operators.CompareString(result.From.Trim(), "", false) == 0)
		{
			result.From = "NAME@gmail.com";
			All.f.StringWriteFN(All.A.FN, "eMail", result.From);
			result.ErrCode = 63;
		}
		Coding coding = new Coding();
		result.Pass = All.f.StringGetFn(All.A.FN, "Pass");
		if (Operators.CompareString(result.Pass.Trim(), "", false) == 0)
		{
			result.Pass = "0123";
			All.f.StringWriteFN(All.A.FN, "Pass", coding.Cod(result.Pass));
			result.ErrCode = 63;
		}
		else
		{
			result.Pass = coding.DeCod(result.Pass);
		}
		result.Port = All.f.IntegerGetFn(All.A.FN, "Port");
		if (result.Port == 0)
		{
			result.Port = 587;
			All.f.IntigerWriteFN(All.A.FN, "Port", result.Port);
			result.ErrCode = 63;
		}
		result.Host = All.f.StringGetFn(All.A.FN, "HostSMTP");
		if (Operators.CompareString(result.Host.Trim(), "", false) == 0)
		{
			result.Host = "smtp.gmail.com";
			All.f.StringWriteFN(All.A.FN, "HostSMTP", result.Host);
			result.ErrCode = 63;
		}
		result.TimeOut = All.f.IntegerGetFn(All.A.FN, "TimeOut");
		if (result.TimeOut < 3000)
		{
			result.TimeOut = 5000;
			All.f.IntigerWriteFN(All.A.FN, "TimeOut", result.TimeOut);
			result.ErrCode = 63;
		}
		if (result.ErrCode == 63)
		{
			All.f.IntigerWriteFN(All.A.FN, "EnableSSL", 1);
		}
		if (All.f.IntegerGetFn(All.A.FN, "EnableSSL") > 0)
		{
			result.EnableSSL = true;
		}
		else
		{
			result.EnableSSL = false;
		}
		if (result.ErrCode == 63)
		{
			All.f.IntigerWriteFN(All.A.FN, "UseDefaultCredentials", 0);
		}
		if (All.f.IntegerGetFn(All.A.FN, "UseDefaultCredentials") > 0)
		{
			result.UseDefaultCredentials = true;
		}
		else
		{
			result.UseDefaultCredentials = false;
		}
		if (result.ErrCode == 63)
		{
			All.f.IntigerWriteFN(All.A.FN, "SendToEmail", 0);
		}
		if (All.f.IntegerGetFn(All.A.FN, "SendToEmail") > 0)
		{
			result.SendToEmail = true;
		}
		else
		{
			result.SendToEmail = false;
		}
		return result;
	}

	public bool SaveEmail(TypMail e)
	{
		All.f.StringWriteFN(All.A.FN, "eMail", e.From);
		Coding coding = new Coding();
		e.Pass = coding.Cod(e.Pass);
		All.f.StringWriteFN(All.A.FN, "Pass", e.Pass);
		All.f.IntigerWriteFN(All.A.FN, "Port", e.Port);
		All.f.StringWriteFN(All.A.FN, "HostSMTP", e.Host);
		All.f.IntigerWriteFN(All.A.FN, "TimeOut", e.TimeOut);
		int val = (e.EnableSSL ? 1 : 0);
		All.f.IntigerWriteFN(All.A.FN, "EnableSSL", val);
		val = (e.UseDefaultCredentials ? 1 : 0);
		All.f.IntigerWriteFN(All.A.FN, "UseDefaultCredentials", val);
		return true;
	}
}
