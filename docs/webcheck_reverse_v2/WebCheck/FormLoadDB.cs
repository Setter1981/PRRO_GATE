using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Amazon;
using Amazon.Runtime.CredentialManagement;
using Amazon.S3;
using Amazon.S3.Model;
using Ionic.Zip;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

[DesignerGenerated]
public class FormLoadDB : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ClB")]
	private Button _ClB;

	private TypLoadDB UpLoadT;

	internal virtual Button ClB
	{
		[CompilerGenerated]
		get
		{
			return _ClB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ClB_Click;
			Button clB = _ClB;
			if (clB != null)
			{
				clB.Click -= value2;
			}
			_ClB = value;
			clB = _ClB;
			if (clB != null)
			{
				clB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ErT")]
	internal virtual TextBox ErT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("LastDB")]
	internal virtual Label LastDB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		this.ClB = new System.Windows.Forms.Button();
		this.ErT = new System.Windows.Forms.TextBox();
		this.LastDB = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.ClB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ClB.Location = new System.Drawing.Point(12, 181);
		this.ClB.Name = "ClB";
		this.ClB.Size = new System.Drawing.Size(610, 44);
		this.ClB.TabIndex = 0;
		this.ClB.Text = "Закрити";
		this.ClB.UseVisualStyleBackColor = true;
		this.ErT.BackColor = System.Drawing.SystemColors.Window;
		this.ErT.Enabled = false;
		this.ErT.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ErT.Location = new System.Drawing.Point(12, 12);
		this.ErT.Multiline = true;
		this.ErT.Name = "ErT";
		this.ErT.ReadOnly = true;
		this.ErT.Size = new System.Drawing.Size(610, 123);
		this.ErT.TabIndex = 1;
		this.ErT.Text = "Чекайте, йде завантаження даних...";
		this.ErT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.LastDB.AutoSize = true;
		this.LastDB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LastDB.Location = new System.Drawing.Point(8, 138);
		this.LastDB.Name = "LastDB";
		this.LastDB.Size = new System.Drawing.Size(276, 20);
		this.LastDB.TabIndex = 10;
		this.LastDB.Text = "Останнє успішне завантаження";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(634, 237);
		base.ControlBox = false;
		base.Controls.Add(this.LastDB);
		base.Controls.Add(this.ErT);
		base.Controls.Add(this.ClB);
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormLoadDB";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Завантаження даних";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormLoadDB(TypLoadDB e)
	{
		base.Load += LoadDB_Load;
		UpLoadT = e;
		InitializeComponent();
	}

	private void LoadDB_Load(object sender, EventArgs e)
	{
		Show();
		Application.DoEvents();
		if (UpLoadT.Online)
		{
			ClB.Enabled = false;
			ErT.Text = "Чекайте, йде завантаження даних...";
			LastDB.Visible = false;
			Application.DoEvents();
			string pathN = All.MyDoc() + "\\WebCheck\\Archive\\" + UpLoadT.FN + "\\" + UpLoadT.FN + ".db";
			if (TestBackup(pathN))
			{
				UpdateTime();
				ClB.Enabled = true;
				Close();
			}
			else
			{
				ErT.Text = "Виникла помилка ";
				LastDB.Text = "Остання успішне оновлення: " + UpLoadT.Update;
				LastDB.Visible = true;
				ClB.Enabled = true;
			}
		}
		else
		{
			ClB.Enabled = true;
			ErT.Text = "Доступ заборонено";
			LastDB.Text = "Остання успішне оновлення: " + UpLoadT.Update;
			LastDB.Visible = true;
		}
	}

	private void ClB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void UpdateTime()
	{
		AccountantОffice accountantОffice = new AccountantОffice();
		int num = accountantОffice.IndexKeysTin(UpLoadT.Tin, UpLoadT.FN);
		All.ArS.StringWriteFN(UpLoadT.Tin, accountantОffice.NameKeyINI("UP", Conversions.ToInteger(num.ToString())), "S " + DateTime.Now.ToString());
	}

	private bool TestBackup(string PathN)
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + UpLoadT.FN + ".db";
		bool result;
		if (!File.Exists(text))
		{
			string text2 = All.PersonalTemp() + "s3.txt";
			if (!File.Exists(text2))
			{
				DownLoadFileS3(text2);
			}
			Coding coding = new Coding();
			IniHGB iniHGB = new IniHGB(text2);
			string keyId = coding.DeCod(iniHGB.GetString("AWS", "KeyId"));
			string secret = coding.DeCod(iniHGB.GetString("AWS", "Secret"));
			WriteProfile(keyId, secret);
			string f = UpLoadT.FN.Trim();
			string t = UpLoadT.Tin.Trim();
			NP nP = default(NP);
			if (!nP.FileArchive(ref f, ref t))
			{
				result = false;
				goto IL_0136;
			}
			if (DownLoadZip(f, t))
			{
				f = All.MyDoc() + "\\WebCheck\\Backup\\" + f + ".zip";
				if (File.Exists(f))
				{
					FileSystem.DeleteFile(f);
				}
			}
		}
		if (File.Exists(text))
		{
			try
			{
				File.Copy(text, PathN, overwrite: true);
				FileSystem.DeleteFile(text);
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		else
		{
			result = false;
		}
		goto IL_0136;
		IL_0136:
		return result;
	}

	internal bool DownLoadZip(string Name, string Key)
	{
		AmazonS3Client amazonS3Client = new AmazonS3Client(RegionEndpoint.EUWest2);
		if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Backup\\"))
		{
			Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Backup\\");
		}
		GetObjectRequest getObjectRequest = new GetObjectRequest();
		getObjectRequest.BucketName = "webchekzipfns";
		getObjectRequest.Key = Name + ".zip";
		_ = null;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + Name + ".zip";
		bool result;
		try
		{
			GetObjectResponse @object = amazonS3Client.GetObject(getObjectRequest);
			@object.WriteResponseStreamToFile(text);
			if (@object.HttpStatusCode != HttpStatusCode.OK)
			{
				result = false;
				goto IL_00ec;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_00ec;
		}
		using (ZipFile zipFile = ZipFile.Read(text))
		{
			try
			{
				string path = All.MyDoc() + "\\WebCheck\\Backup\\";
				zipFile.Password = Key;
				zipFile.ExtractAll(path);
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_00ec;
			}
		}
		result = true;
		goto IL_00ec;
		IL_00ec:
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

	private bool DownLoadFileS3(string fl)
	{
		string address = "https://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
		bool result;
		try
		{
			if (File.Exists(fl))
			{
				FileSystem.DeleteFile(fl);
			}
			new Network().DownloadFile(address, fl);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0034;
		}
		result = true;
		goto IL_0034;
		IL_0034:
		return result;
	}
}
