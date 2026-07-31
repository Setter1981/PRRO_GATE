using System;
using System.IO;
using System.Net;
using Amazon;
using Amazon.Runtime.CredentialManagement;
using Amazon.S3;
using Amazon.S3.Model;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

internal class SendFilePDF
{
	private AmazonS3Client s3Client;

	public SendFilePDF()
	{
		s3Client = new AmazonS3Client(RegionEndpoint.EUWest2);
	}

	internal TypErrStr SendPDF(string PathPdf, string NameFile)
	{
		TypErrStr result = default(TypErrStr);
		result.ReturnStr = "";
		result.errCode = 0;
		result.errStr = "";
		string text = All.PersonalTemp() + "s3.txt";
		if (!File.Exists(text))
		{
			DownLoadFile(text);
		}
		Coding coding = new Coding();
		IniHGB iniHGB = new IniHGB(text);
		string keyId = coding.DeCod(iniHGB.GetString("AWS", "KeyId"));
		string secret = coding.DeCod(iniHGB.GetString("AWS", "Secret"));
		WriteProfile(keyId, secret);
		PutObjectRequest putObjectRequest = new PutObjectRequest();
		putObjectRequest.BucketName = "che.ck.ua";
		putObjectRequest.Key = NameFile;
		putObjectRequest.FilePath = PathPdf;
		putObjectRequest.ContentType = "application/pdf";
		_ = null;
		try
		{
			PutObjectResponse putObjectResponse = s3Client.PutObject(putObjectRequest);
			if (putObjectResponse.HttpStatusCode != HttpStatusCode.OK)
			{
				result.errCode = 99;
				result.errStr = putObjectResponse.HttpStatusCode.ToString();
				if (File.Exists(text))
				{
					FileSystem.DeleteFile(text);
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 99;
			result.errStr = "Ошибка отправки PDF: " + ex2.Message;
			if (File.Exists(text))
			{
				FileSystem.DeleteFile(text);
			}
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool WriteProfile(string keyId, string secret, string profileName = "default")
	{
		bool result;
		try
		{
			CredentialProfileOptions credentialProfileOptions = new CredentialProfileOptions();
			credentialProfileOptions.AccessKey = keyId;
			credentialProfileOptions.SecretKey = secret;
			CredentialProfile profile = new CredentialProfile(profileName, credentialProfileOptions);
			new NetSDKCredentialsFile().RegisterProfile(profile);
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

	private bool DownLoadFile(string fl)
	{
		string address = "https://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
		bool result;
		try
		{
			if (File.Exists(fl))
			{
				FileSystem.DeleteFile(fl);
			}
			MyProject.Computer.Network.DownloadFile(address, fl);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0039;
		}
		result = true;
		goto IL_0039;
		IL_0039:
		return result;
	}
}
